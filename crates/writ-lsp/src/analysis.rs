use std::collections::HashMap;
use std::path::Path;

use lsp_types::Diagnostic;
use writ::lexer::Lexer;
use writ::parser::{Parser, Stmt};
use writ::resolver::{collect_import_paths, extract_exports};
use writ::types::{Type, TypeChecker};

use crate::diagnostics::{
    lex_error_to_diagnostic, parse_error_to_diagnostic, type_error_to_diagnostic,
};

/// Result of running the analysis pipeline on a single file.
pub struct AnalysisResult {
    /// The parsed AST (if lexing and parsing succeeded).
    pub stmts: Option<Vec<Stmt>>,
    /// The type checker state after analysis (for completions, hover, etc.).
    /// Available even when type errors occurred.
    pub type_checker: Option<TypeChecker>,
    /// All diagnostics collected from lex, parse, and type-check stages.
    pub diagnostics: Vec<Diagnostic>,
}

/// Runs the full analysis pipeline: lex → parse → type-check.
///
/// Collects all diagnostics from each stage. The pipeline continues as far
/// as possible even when earlier stages produce errors.
///
/// When `file_path` points to a real file on disk, import statements are
/// resolved by reading `.writ` files relative to the file's directory.
pub fn analyze(source: &str, file_path: &str) -> AnalysisResult {
    let mut diagnostics = Vec::new();

    // Stage 1: Lex
    let mut lexer = Lexer::with_file(source, file_path);
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(err) => {
            diagnostics.push(lex_error_to_diagnostic(&err));
            return AnalysisResult {
                stmts: None,
                type_checker: None,
                diagnostics,
            };
        }
    };

    // Stage 2: Parse
    let mut parser = Parser::new(tokens);
    let stmts = match parser.parse_program() {
        Ok(stmts) => stmts,
        Err(err) => {
            diagnostics.push(parse_error_to_diagnostic(&err));
            return AnalysisResult {
                stmts: None,
                type_checker: None,
                diagnostics,
            };
        }
    };

    // Stage 3: Resolve imports from disk and type-check
    let mut checker = TypeChecker::new();

    // Discover import paths and resolve them from disk.
    let import_paths = collect_import_paths(&stmts);
    if !import_paths.is_empty() {
        let base_dir = Path::new(file_path).parent();
        for module_path in &import_paths {
            if let Some(exports) = resolve_module_from_disk(module_path, base_dir) {
                checker.register_module(module_path, exports);
            }
        }
    }

    let type_errors = checker.check_program_collecting(&stmts);
    for err in &type_errors {
        diagnostics.push(type_error_to_diagnostic(err));
    }

    AnalysisResult {
        stmts: Some(stmts),
        type_checker: Some(checker),
        diagnostics,
    }
}

/// Resolves a module path to disk, analyzes it, and returns its exports.
///
/// The module path (e.g. `"items/weapon"`) is resolved relative to `base_dir`
/// by appending `.writ`. Returns `None` if the file doesn't exist or can't
/// be analyzed.
fn resolve_module_from_disk(
    module_path: &str,
    base_dir: Option<&Path>,
) -> Option<HashMap<String, Type>> {
    let base = base_dir?;
    let file = base.join(format!("{module_path}.writ"));
    let source = std::fs::read_to_string(&file).ok()?;
    let file_path_str = file.to_string_lossy().into_owned();

    // Lex + parse the dependency (don't recurse into its imports to avoid cycles).
    let mut lexer = Lexer::with_file(&source, &file_path_str);
    let tokens = lexer.tokenize().ok()?;
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().ok()?;

    // Type-check the dependency to get accurate types.
    let mut checker = TypeChecker::new();
    // Ignore type errors in the dependency — we still extract what we can.
    let _ = checker.check_program_collecting(&stmts);

    // Extract exported names and their types.
    let exports = extract_exports(&stmts, &checker);
    if exports.is_empty() {
        None
    } else {
        Some(exports)
    }
}
