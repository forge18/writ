pub(super) use std::collections::HashMap;

pub(super) use crate::lexer::Span;
pub(super) use crate::parser::{
    ArrayElement, AssignOp, BinaryOp, CallArg, ClassDecl, Decl, DeclKind, DictElement, ElseBranch,
    EnumDecl, Expr, ExprKind, FuncDecl, ImportDecl, InterpolationSegment, LambdaBody, Literal,
    Stmt, StmtKind, StructDecl, TraitDecl, TypeExpr, UnaryOp, Visibility, WhenBody, WhenPattern,
    WildcardImportDecl,
};

pub(super) use super::env::{Mutability, TypeEnv, VarInfo};
pub(super) use super::error::TypeError;
pub(super) use super::module_registry::ModuleRegistry;
pub(super) use super::registry::{
    ClassInfo, EnumInfo, FieldInfo, MethodInfo, StructInfo, TraitInfo, TypeRegistry,
};
pub(super) use super::suggestions;
pub(super) use super::typed_ast::TypedStmt;
pub(super) use super::types::Type;

/// Type checker for the Writ language.
///
/// Walks the AST produced by `writ-parser` and validates that all
/// expressions and statements are type-correct. Reports errors as
/// [`TypeError`] values with source location information.
pub struct TypeChecker {
    env: TypeEnv,
    registry: TypeRegistry,
    module_registry: ModuleRegistry,
    /// Maps wildcard import aliases to module paths.
    namespace_aliases: HashMap<String, String>,
    /// The expected return type of the function currently being checked.
    /// `None` when outside any function body.
    current_return_type: Option<Type>,
    /// The class whose method body is currently being type-checked.
    /// Used to resolve `super.method(...)` calls.
    current_class: Option<String>,
    /// Uninstantiated generic class templates, keyed by name.
    generic_classes: HashMap<String, ClassDecl>,
    /// Uninstantiated generic struct templates, keyed by name.
    generic_structs: HashMap<String, StructDecl>,
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeChecker {
    /// Creates a new type checker with an empty global scope.
    pub fn new() -> Self {
        Self {
            env: TypeEnv::new(),
            registry: TypeRegistry::new(),
            module_registry: ModuleRegistry::new(),
            namespace_aliases: HashMap::new(),
            current_return_type: None,
            current_class: None,
            generic_classes: HashMap::new(),
            generic_structs: HashMap::new(),
        }
    }

    /// Pre-register a module's exported types before type-checking begins.
    pub fn register_module(&mut self, path: &str, exports: HashMap<String, Type>) {
        self.module_registry.register_module(path, exports);
    }

    /// Register a host-provided type that is globally available without import.
    pub fn register_host_type(&mut self, name: &str, methods: Vec<MethodInfo>) {
        self.registry.register_class(ClassInfo {
            name: name.to_string(),
            fields: Vec::new(),
            methods,
            parent: None,
            traits: Vec::new(),
        });
    }

    /// Register a host-provided function that is globally available without import.
    ///
    /// This makes the function's type signature known to the type checker so
    /// that calls to it (e.g. stdlib functions like `print`) pass validation.
    pub fn register_host_function(&mut self, name: &str, params: Vec<Type>, return_type: Type) {
        self.env.define(
            name,
            VarInfo {
                ty: Type::Function {
                    params,
                    return_type: Box::new(return_type),
                },
                mutability: Mutability::Immutable,
            },
        );
    }

    /// Returns a reference to the type environment (for LSP queries).
    pub fn env(&self) -> &TypeEnv {
        &self.env
    }

    /// Returns a reference to the type registry (for LSP queries).
    pub fn registry(&self) -> &TypeRegistry {
        &self.registry
    }

    /// Returns a reference to the module registry (for LSP queries).
    pub fn module_registry(&self) -> &ModuleRegistry {
        &self.module_registry
    }

    /// Returns the namespace aliases (for LSP queries).
    pub fn namespace_aliases(&self) -> &HashMap<String, String> {
        &self.namespace_aliases
    }

    /// Type-checks a top-level declaration.
    pub fn check_decl(&mut self, decl: &Decl) -> Result<(), TypeError> {
        match &decl.kind {
            DeclKind::Stmt(stmt) => self.check_stmt(stmt),
            DeclKind::Func(func) => self.check_func_decl(func, &decl.span),
            DeclKind::Class(class_decl) => self.check_class_decl(class_decl, &decl.span),
            DeclKind::Trait(trait_decl) => self.check_trait_decl(trait_decl, &decl.span),
            DeclKind::Enum(enum_decl) => self.check_enum_decl(enum_decl, &decl.span),
            DeclKind::Struct(struct_decl) => self.check_struct_decl(struct_decl, &decl.span),
            DeclKind::Import(import) => self.check_import(import, &decl.span),
            DeclKind::WildcardImport(import) => self.check_wildcard_import(import, &decl.span),
            DeclKind::Export(inner) => self.check_decl(inner),
        }
    }

    /// Type-checks a sequence of statements.
    ///
    /// Uses two passes: the first registers all type declarations (class,
    /// trait, enum) so that forward references resolve correctly. The second
    /// pass performs full type checking including method bodies.
    pub fn check_program(&mut self, stmts: &[Stmt]) -> Result<(), TypeError> {
        // Pass 1: Register all type declarations (signatures only).
        for stmt in stmts {
            self.register_type_if_decl(stmt)?;
        }
        // Pass 2: Full type checking.
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }

    /// Type-checks a program and collects all errors instead of stopping at the first.
    ///
    /// Returns the list of errors (empty if the program is well-typed).
    /// The type checker state is still updated, so `env()` and `registry()` remain
    /// usable for LSP queries even when errors are present.
    pub fn check_program_collecting(&mut self, stmts: &[Stmt]) -> Vec<TypeError> {
        let mut errors = Vec::new();
        // Pass 1: Register all type declarations (signatures only).
        for stmt in stmts {
            if let Err(e) = self.register_type_if_decl(stmt) {
                errors.push(e);
            }
        }
        // Pass 2: Full type checking.
        for stmt in stmts {
            if let Err(e) = self.check_stmt(stmt) {
                errors.push(e);
            }
        }
        Self::suppress_cascading_errors(errors)
    }

    /// Filters out errors that are likely caused by earlier root errors.
    ///
    /// Heuristic: if an error mentions an undefined variable/type, that name
    /// is "poisoned". Subsequent errors whose message references a poisoned
    /// name are suppressed, since they are probably downstream consequences
    /// of the original error.
    fn suppress_cascading_errors(errors: Vec<TypeError>) -> Vec<TypeError> {
        use std::collections::HashSet;

        let mut poisoned: HashSet<String> = HashSet::new();
        let mut result = Vec::new();

        for error in errors {
            // Check if this error is a downstream consequence of an earlier error
            let is_cascading =
                !poisoned.is_empty() && poisoned.iter().any(|name| error.message.contains(name));

            if is_cascading {
                continue;
            }

            // Extract poisoned names from "undefined variable 'x'" or "undefined type 'X'" errors
            if (error.message.starts_with("undefined variable '")
                || error.message.starts_with("undefined type '")
                || error.message.starts_with("undefined function '"))
                && let Some(start) = error.message.find('\'')
                && let Some(end) = error.message[start + 1..].find('\'')
            {
                let name = &error.message[start + 1..start + 1 + end];
                poisoned.insert(name.to_string());
            }

            result.push(error);
        }

        result
    }

    /// Type-checks a program and returns type-annotated statement wrappers.
    ///
    /// Each [`TypedStmt`] carries the checker-inferred type of the statement's
    /// primary expression. The bytecode compiler consumes this to seed register
    /// types without re-inferring, eliminating drift between the two type systems.
    ///
    /// Returns `Err` on the first type error (same behaviour as [`check_program`](Self::check_program)).
    pub fn check_program_typed(&mut self, stmts: &[Stmt]) -> Result<Vec<TypedStmt>, TypeError> {
        // Pass 1: Register all type declarations (signatures only).
        for stmt in stmts {
            self.register_type_if_decl(stmt)?;
        }
        // Pass 2: Full type checking + annotation.
        let mut typed = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            let expr_type = self.check_stmt_typed(stmt)?;
            typed.push(TypedStmt {
                stmt: stmt.clone(),
                expr_type,
            });
        }
        Ok(typed)
    }

    /// Type-checks a statement and returns the inferred type of its primary expression.
    ///
    /// For `Let`/`Var`/`Const` the returned type is the initializer's type.
    /// For `ExprStmt` it is the expression's type. All other statements return
    /// [`Type::Void`].
    fn check_stmt_typed(&mut self, stmt: &Stmt) -> Result<Type, TypeError> {
        match &stmt.kind {
            StmtKind::Let { initializer, .. } | StmtKind::Var { initializer, .. } => {
                self.check_stmt(stmt)?;
                self.infer_expr(initializer)
            }
            StmtKind::Const { initializer, .. } => {
                self.check_stmt(stmt)?;
                self.infer_expr(initializer)
            }
            StmtKind::ExprStmt(expr) => {
                self.check_stmt(stmt)?;
                self.infer_expr(expr)
            }
            _ => {
                self.check_stmt(stmt)?;
                Ok(Type::Void)
            }
        }
    }
}

mod collections;
mod decls;
mod exprs;
mod registry;
mod stmts;
mod type_decls;
mod types;

pub(super) fn infer_literal(lit: &Literal) -> Type {
    match lit {
        Literal::Int(_) => Type::Int,
        Literal::Float(_) => Type::Float,
        Literal::String(_) => Type::Str,
        Literal::Bool(_) => Type::Bool,
        Literal::Null => Type::Optional(Box::new(Type::Unknown)),
    }
}

/// Extracts the expression from a [`CallArg`], whether positional or named.
pub(super) fn call_arg_expr(arg: &CallArg) -> &Expr {
    match arg {
        CallArg::Positional(expr) => expr,
        CallArg::Named { value, .. } => value,
    }
}

/// Returns `true` if every code path through `stmts` ends with a `return`.
pub(super) fn returns_on_all_paths(stmts: &[Stmt]) -> bool {
    let Some(last) = stmts.last() else {
        return false;
    };
    match &last.kind {
        StmtKind::Return(_) => true,
        StmtKind::Block(inner) => returns_on_all_paths(inner),
        StmtKind::If {
            then_block,
            else_branch: Some(else_branch),
            ..
        } => {
            let then_returns = returns_on_all_paths(then_block);
            let else_returns = match else_branch {
                ElseBranch::ElseBlock(stmts) => returns_on_all_paths(stmts),
                ElseBranch::ElseIf(stmt) => match &stmt.kind {
                    StmtKind::If {
                        then_block,
                        else_branch,
                        ..
                    } => {
                        let inner_then = returns_on_all_paths(then_block);
                        let inner_else = else_branch
                            .as_ref()
                            .map(|b| match b {
                                ElseBranch::ElseBlock(s) => returns_on_all_paths(s),
                                ElseBranch::ElseIf(s) => {
                                    returns_on_all_paths(&[s.as_ref().clone()])
                                }
                            })
                            .unwrap_or(false);
                        inner_then && inner_else
                    }
                    _ => false,
                },
            };
            then_returns && else_returns
        }
        _ => false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn test_span() -> Span {
        Span {
            file: String::new(),
            line: 1,
            column: 1,
            length: 0,
        }
    }

    #[test]
    fn resolve_all_primitive_types() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let cases = [
            ("int", Type::Int),
            ("float", Type::Float),
            ("bool", Type::Bool),
            ("string", Type::Str),
            ("void", Type::Void),
        ];
        for (name, expected) in cases {
            let result = checker.resolve_type_expr(&TypeExpr::Simple(name.to_string()), &span);
            assert_eq!(result.unwrap(), expected, "failed for type '{name}'");
        }
    }

    #[test]
    fn resolve_unknown_type_name() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let result = checker.resolve_type_expr(&TypeExpr::Simple("Foo".to_string()), &span);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown type"));
    }

    #[test]
    fn resolve_generic_type_unsupported() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        // Unknown generic type names should produce an error.
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "List".to_string(),
                args: vec![TypeExpr::Simple("int".to_string())],
            },
            &span,
        );
        assert!(result.is_err());
    }

    #[test]
    fn resolve_array_type() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "Array".to_string(),
                args: vec![TypeExpr::Simple("int".to_string())],
            },
            &span,
        );
        assert_eq!(result.unwrap(), Type::Array(Box::new(Type::Int)));
    }

    #[test]
    fn resolve_dictionary_type() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "Dictionary".to_string(),
                args: vec![
                    TypeExpr::Simple("string".to_string()),
                    TypeExpr::Simple("int".to_string()),
                ],
            },
            &span,
        );
        assert_eq!(
            result.unwrap(),
            Type::Dictionary(Box::new(Type::Str), Box::new(Type::Int))
        );
    }

    #[test]
    fn resolve_tuple_type() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let result = checker.resolve_type_expr(
            &TypeExpr::Tuple(vec![
                TypeExpr::Simple("int".to_string()),
                TypeExpr::Simple("float".to_string()),
            ]),
            &span,
        );
        assert_eq!(result.unwrap(), Type::Tuple(vec![Type::Int, Type::Float]));
    }

    #[test]
    fn resolve_result_type() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "Result".to_string(),
                args: vec![TypeExpr::Simple("int".to_string())],
            },
            &span,
        );
        assert_eq!(result.unwrap(), Type::Result(Box::new(Type::Int)));
    }

    #[test]
    fn resolve_optional_type() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "Optional".to_string(),
                args: vec![TypeExpr::Simple("string".to_string())],
            },
            &span,
        );
        assert_eq!(result.unwrap(), Type::Optional(Box::new(Type::Str)));
    }

    #[test]
    fn resolve_user_defined_generic_struct() {
        use crate::parser::{FieldDecl, StructDecl, Visibility};

        let mut checker = TypeChecker::new();
        let span = test_span();

        // Register a generic struct template: struct Box<T> { value: T }
        let template = StructDecl {
            name: "Box".to_string(),
            type_params: vec!["T".to_string()],
            fields: vec![FieldDecl {
                name: "value".to_string(),
                type_annotation: TypeExpr::Simple("T".to_string()),
                default: None,
                visibility: Visibility::Public,
                setter: None,
            }],
            methods: vec![],
        };
        checker.generic_structs.insert("Box".to_string(), template);

        // Instantiate Box<int>
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "Box".to_string(),
                args: vec![TypeExpr::Simple("int".to_string())],
            },
            &span,
        );
        assert_eq!(result.unwrap(), Type::Struct("Box__int".to_string()));

        // Verify the monomorphic struct was registered.
        assert!(checker.registry.get_struct("Box__int").is_some());
        let info = checker.registry.get_struct("Box__int").unwrap();
        assert_eq!(info.fields[0].ty, Type::Int);
    }

    #[test]
    fn resolve_user_defined_generic_class() {
        use crate::parser::{ClassDecl, FieldDecl, Visibility};

        let mut checker = TypeChecker::new();
        let span = test_span();

        let template = ClassDecl {
            name: "Stack".to_string(),
            type_params: vec!["T".to_string()],
            extends: None,
            traits: vec![],
            fields: vec![FieldDecl {
                name: "top".to_string(),
                type_annotation: TypeExpr::Simple("T".to_string()),
                default: None,
                visibility: Visibility::Public,
                setter: None,
            }],
            methods: vec![],
            where_clauses: vec![],
        };
        checker
            .generic_classes
            .insert("Stack".to_string(), template);

        // Instantiate Stack<string>
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "Stack".to_string(),
                args: vec![TypeExpr::Simple("string".to_string())],
            },
            &span,
        );
        assert_eq!(result.unwrap(), Type::Class("Stack__string".to_string()));
        assert!(checker.registry.get_class("Stack__string").is_some());
        let info = checker.registry.get_class("Stack__string").unwrap();
        assert_eq!(info.fields[0].ty, Type::Str);
    }

    #[test]
    fn resolve_generic_wrong_arity_is_error() {
        use crate::parser::{FieldDecl, StructDecl, Visibility};

        let mut checker = TypeChecker::new();
        let span = test_span();

        let template = StructDecl {
            name: "Pair".to_string(),
            type_params: vec!["A".to_string(), "B".to_string()],
            fields: vec![
                FieldDecl {
                    name: "first".to_string(),
                    type_annotation: TypeExpr::Simple("A".to_string()),
                    default: None,
                    visibility: Visibility::Public,
                    setter: None,
                },
                FieldDecl {
                    name: "second".to_string(),
                    type_annotation: TypeExpr::Simple("B".to_string()),
                    default: None,
                    visibility: Visibility::Public,
                    setter: None,
                },
            ],
            methods: vec![],
        };
        checker.generic_structs.insert("Pair".to_string(), template);

        // Only one arg supplied to a two-param generic -- should error.
        let result = checker.resolve_type_expr(
            &TypeExpr::Generic {
                name: "Pair".to_string(),
                args: vec![TypeExpr::Simple("int".to_string())],
            },
            &span,
        );
        assert!(result.is_err());
    }

    #[test]
    fn resolve_generic_cached_on_second_use() {
        use crate::parser::{FieldDecl, StructDecl, Visibility};

        let mut checker = TypeChecker::new();
        let span = test_span();

        let template = StructDecl {
            name: "Wrap".to_string(),
            type_params: vec!["T".to_string()],
            fields: vec![FieldDecl {
                name: "inner".to_string(),
                type_annotation: TypeExpr::Simple("T".to_string()),
                default: None,
                visibility: Visibility::Public,
                setter: None,
            }],
            methods: vec![],
        };
        checker.generic_structs.insert("Wrap".to_string(), template);

        let te = TypeExpr::Generic {
            name: "Wrap".to_string(),
            args: vec![TypeExpr::Simple("float".to_string())],
        };

        let r1 = checker.resolve_type_expr(&te, &span).unwrap();
        let r2 = checker.resolve_type_expr(&te, &span).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r1, Type::Struct("Wrap__float".to_string()));
    }

    #[test]
    fn assignment_to_let_is_error() {
        let mut checker = TypeChecker::new();
        checker.env.define(
            "x",
            VarInfo {
                ty: Type::Int,
                mutability: Mutability::Immutable,
            },
        );
        let target = Expr {
            kind: ExprKind::Identifier("x".to_string()),
            span: Span {
                file: String::new(),
                line: 1,
                column: 1,
                length: 1,
            },
        };
        let value = Expr {
            kind: ExprKind::Literal(Literal::Int(5)),
            span: Span {
                file: String::new(),
                line: 1,
                column: 5,
                length: 1,
            },
        };
        let result = checker.check_assignment(&target, &AssignOp::Assign, &value);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("immutable"));
    }

    #[test]
    fn assignment_to_const_is_error() {
        let mut checker = TypeChecker::new();
        checker.env.define(
            "MAX",
            VarInfo {
                ty: Type::Int,
                mutability: Mutability::Constant,
            },
        );
        let target = Expr {
            kind: ExprKind::Identifier("MAX".to_string()),
            span: Span {
                file: String::new(),
                line: 1,
                column: 1,
                length: 3,
            },
        };
        let value = Expr {
            kind: ExprKind::Literal(Literal::Int(5)),
            span: Span {
                file: String::new(),
                line: 1,
                column: 7,
                length: 1,
            },
        };
        let result = checker.check_assignment(&target, &AssignOp::Assign, &value);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("constant"));
    }

    #[test]
    fn compound_assignment_non_numeric_is_error() {
        let mut checker = TypeChecker::new();
        checker.env.define(
            "s",
            VarInfo {
                ty: Type::Str,
                mutability: Mutability::Mutable,
            },
        );
        let target = Expr {
            kind: ExprKind::Identifier("s".to_string()),
            span: Span {
                file: String::new(),
                line: 1,
                column: 1,
                length: 1,
            },
        };
        let value = Expr {
            kind: ExprKind::Literal(Literal::Int(1)),
            span: Span {
                file: String::new(),
                line: 1,
                column: 6,
                length: 1,
            },
        };
        let result = checker.check_assignment(&target, &AssignOp::AddAssign, &value);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("arithmetic assignment")
        );
    }

    #[test]
    fn struct_self_referential_method_does_not_panic() {
        // A struct whose method takes itself as a parameter or returns itself
        // must not panic during type registration.
        let src = r#"
struct Point {
    public x: float = 0.0
    public y: float = 0.0

    func add(other: Point) -> Point {
        return Point(self.x + other.x, self.y + other.y)
    }
}
"#;
        let mut lexer = crate::lexer::Lexer::new(src);
        let tokens = lexer.tokenize().expect("lex");
        let mut parser = crate::parser::Parser::new(tokens);
        let stmts = parser.parse_program().expect("parse");
        let mut checker = TypeChecker::new();
        // Must not panic; errors are acceptable (e.g. unresolved Point constructor)
        let _ = checker.check_program_collecting(&stmts);
        // Point must be in the registry after registration.
        assert!(
            checker.registry().get_struct("Point").is_some(),
            "Point should be registered even when method signatures reference it"
        );
    }

    #[test]
    fn resolve_qualified_type_from_namespace() {
        use crate::parser::{Decl, DeclKind, WildcardImportDecl};
        use std::collections::HashMap;

        let mut checker = TypeChecker::new();
        let span = test_span();

        // Register a module with an Enemy class export.
        let mut exports = HashMap::new();
        exports.insert("Enemy".to_string(), Type::Class("Enemy".to_string()));
        checker.register_module("entities/enemy", exports);

        // Process `import * as enemy from "entities/enemy"`.
        checker
            .check_decl(&Decl {
                kind: DeclKind::WildcardImport(WildcardImportDecl {
                    alias: "enemy".to_string(),
                    from: "entities/enemy".to_string(),
                }),
                span: span.clone(),
            })
            .unwrap();

        let result = checker.resolve_type_expr(
            &TypeExpr::Qualified {
                namespace: "enemy".to_string(),
                name: "Enemy".to_string(),
            },
            &span,
        );
        assert_eq!(result.unwrap(), Type::Class("Enemy".to_string()));
    }

    #[test]
    fn resolve_qualified_type_unknown_namespace_errors() {
        let mut checker = TypeChecker::new();
        let span = test_span();
        let result = checker.resolve_type_expr(
            &TypeExpr::Qualified {
                namespace: "unknown".to_string(),
                name: "Foo".to_string(),
            },
            &span,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown namespace"));
    }
}
