//! Automatic module resolution for import statements.
//!
//! When a script contains `import { Foo } from "path/to/module"`, the resolver
//! finds the `.writ` file on disk, compiles it, and loads it into the VM —
//! recursively handling transitive imports. This is analogous to Lua's `require`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::WritError;
use crate::compiler::Compiler;
use crate::lexer::Lexer;
use crate::parser::{Parser, Stmt, StmtKind};
use crate::types::{Type, TypeChecker};
use crate::vm::VM;

/// Resolves, compiles, and caches Writ module files.
pub struct ModuleResolver {
    /// Optional root directory for import path resolution.
    /// When `None`, paths resolve relative to the importing file's parent.
    root_dir: Option<PathBuf>,
    /// Canonical paths of modules that have been fully loaded.
    loaded: HashSet<PathBuf>,
    /// Canonical paths of modules currently being resolved (cycle detection).
    in_progress: HashSet<PathBuf>,
}

impl Default for ModuleResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleResolver {
    pub fn new() -> Self {
        Self {
            root_dir: None,
            loaded: HashSet::new(),
            in_progress: HashSet::new(),
        }
    }

    /// Sets the root directory for import path resolution.
    ///
    /// When set, all import paths resolve relative to this directory.
    /// When unset, paths resolve relative to the importing file's parent.
    pub fn set_root_dir(&mut self, path: impl Into<PathBuf>) {
        self.root_dir = Some(path.into());
    }

    /// Resolves and loads all imports referenced by `stmts`, recursively.
    ///
    /// Each dependency is lexed, parsed, type-checked, compiled, and loaded
    /// into the VM exactly once. Circular imports produce an error.
    pub fn resolve_imports(
        &mut self,
        file_path: &Path,
        stmts: &[Stmt],
        type_checker: &mut TypeChecker,
        vm: &mut VM,
        type_check_enabled: bool,
    ) -> Result<(), WritError> {
        let import_paths = collect_import_paths(stmts);
        if import_paths.is_empty() {
            return Ok(());
        }

        let base_dir = self
            .root_dir
            .clone()
            .or_else(|| file_path.parent().map(Path::to_path_buf));

        for module_path in &import_paths {
            self.resolve_single_module(
                module_path,
                base_dir.as_deref(),
                type_checker,
                vm,
                type_check_enabled,
            )?;
        }

        Ok(())
    }

    /// Resolves a single module path, recursing into its own imports.
    fn resolve_single_module(
        &mut self,
        module_path: &str,
        base_dir: Option<&Path>,
        type_checker: &mut TypeChecker,
        vm: &mut VM,
        type_check_enabled: bool,
    ) -> Result<(), WritError> {
        let file = resolve_path(module_path, base_dir).ok_or_else(|| {
            WritError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("cannot resolve module '{module_path}': file not found"),
            ))
        })?;

        // Already fully loaded — skip.
        if self.loaded.contains(&file) {
            return Ok(());
        }

        // Currently being resolved — circular import.
        if self.in_progress.contains(&file) {
            return Err(WritError::Io(std::io::Error::other(format!(
                "circular import detected: '{module_path}'"
            ))));
        }

        self.in_progress.insert(file.clone());

        let source = std::fs::read_to_string(&file)?;
        let file_str = file.to_string_lossy().into_owned();

        let mut lexer = Lexer::with_file(&source, &file_str);
        let tokens = lexer.tokenize()?;

        let mut parser = Parser::new(tokens);
        let stmts = parser.parse_program()?;

        // Recurse into this dependency's own imports first (depth-first).
        self.resolve_imports(&file, &stmts, type_checker, vm, type_check_enabled)?;

        // Type-check the dependency with the shared type checker.
        // This registers all its classes/structs/enums/traits in the registry.
        if type_check_enabled {
            let _ = type_checker.check_program_collecting(&stmts);
        }

        // Extract exported names and register in the module registry.
        let exports = extract_exports(&stmts, type_checker);
        if !exports.is_empty() {
            type_checker.register_module(module_path, exports);
        }

        // Compile and load bytecode into the VM.
        let mut compiler = Compiler::new();
        compiler.set_native_index(vm.native_fn_index_map().clone());
        compiler.pre_register_classes(&stmts);
        if type_check_enabled {
            let typed_stmts = type_checker.check_program_typed(&stmts)?;
            for typed in &typed_stmts {
                compiler.compile_typed_stmt(typed)?;
            }
        } else {
            for stmt in &stmts {
                compiler.compile_stmt(stmt)?;
            }
        }
        let (chunk, functions, struct_metas, class_metas, enum_metas) = compiler.into_parts();
        vm.load_module(&chunk, &functions, &struct_metas, &class_metas, &enum_metas)?;

        self.in_progress.remove(&file);
        self.loaded.insert(file);

        Ok(())
    }
}

/// Resolves a module path string to a canonical file path.
///
/// Appends `.writ` and resolves relative to `base_dir`. Returns `None` if
/// the file doesn't exist.
fn resolve_path(module_path: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let base = base_dir?;
    let file = base.join(format!("{module_path}.writ"));
    if file.exists() {
        // Canonicalize to deduplicate paths like "./a/../a/mod.writ".
        std::fs::canonicalize(&file).ok()
    } else {
        None
    }
}

/// Collects all unique import module paths from a parsed AST.
pub fn collect_import_paths(stmts: &[Stmt]) -> Vec<String> {
    let mut paths = HashSet::new();
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Import(import) => {
                paths.insert(import.from.clone());
            }
            StmtKind::WildcardImport(import) => {
                paths.insert(import.from.clone());
            }
            StmtKind::Export(inner) => {
                if let StmtKind::Import(import) = &inner.kind {
                    paths.insert(import.from.clone());
                } else if let StmtKind::WildcardImport(import) = &inner.kind {
                    paths.insert(import.from.clone());
                }
            }
            _ => {}
        }
    }
    paths.into_iter().collect()
}

/// Extracts exported names and their resolved types from an analyzed file.
///
/// Walks the AST looking for `export` wrappers and maps the declared name
/// to the type computed by the type checker.
pub fn extract_exports(stmts: &[Stmt], checker: &TypeChecker) -> HashMap<String, Type> {
    let mut exports = HashMap::new();
    let env = checker.env();
    let registry = checker.registry();

    for stmt in stmts {
        if let StmtKind::Export(inner) = &stmt.kind {
            match &inner.kind {
                StmtKind::Func(func) => {
                    if let Some(info) = env.lookup(&func.name) {
                        exports.insert(func.name.clone(), info.ty.clone());
                    }
                }
                StmtKind::Class(class) => {
                    if registry.get_class(&class.name).is_some() {
                        exports.insert(class.name.clone(), Type::Class(class.name.clone()));
                    }
                }
                StmtKind::Trait(trait_decl) => {
                    if registry.get_trait(&trait_decl.name).is_some() {
                        exports.insert(
                            trait_decl.name.clone(),
                            Type::Trait(trait_decl.name.clone()),
                        );
                    }
                }
                StmtKind::Enum(enum_decl) => {
                    if registry.get_enum(&enum_decl.name).is_some() {
                        exports.insert(enum_decl.name.clone(), Type::Enum(enum_decl.name.clone()));
                    }
                }
                StmtKind::Struct(struct_decl) => {
                    if registry.get_struct(&struct_decl.name).is_some() {
                        exports.insert(
                            struct_decl.name.clone(),
                            Type::Struct(struct_decl.name.clone()),
                        );
                    }
                }
                StmtKind::Let { name, .. }
                | StmtKind::Var { name, .. }
                | StmtKind::Const { name, .. } => {
                    if let Some(info) = env.lookup(name) {
                        exports.insert(name.clone(), info.ty.clone());
                    }
                }
                _ => {}
            }
        }
    }
    exports
}
