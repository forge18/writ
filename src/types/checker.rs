use std::collections::HashMap;

use crate::lexer::Span;
use crate::parser::{
    ArrayElement, AssignOp, BinaryOp, CallArg, ClassDecl, Decl, DeclKind, DictElement, ElseBranch,
    EnumDecl, Expr, ExprKind, FuncDecl, ImportDecl, InterpolationSegment, LambdaBody, Literal,
    Stmt, StmtKind, StructDecl, TraitDecl, TypeExpr, UnaryOp, Visibility, WhenBody, WhenPattern,
    WildcardImportDecl,
};

use super::env::{Mutability, TypeEnv, VarInfo};
use super::error::TypeError;
use super::module_registry::ModuleRegistry;
use super::registry::{
    ClassInfo, EnumInfo, FieldInfo, MethodInfo, StructInfo, TraitInfo, TypeRegistry,
};
use super::suggestions;
use super::typed_ast::TypedStmt;
use super::types::Type;

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

    /// Type-checks a single statement.
    pub fn check_stmt(&mut self, stmt: &Stmt) -> Result<(), TypeError> {
        match &stmt.kind {
            StmtKind::Let {
                name,
                type_annotation,
                initializer,
                ..
            } => self.check_variable_decl(
                name,
                type_annotation.as_ref(),
                initializer,
                &stmt.span,
                Mutability::Immutable,
            ),

            StmtKind::Var {
                name,
                type_annotation,
                initializer,
                ..
            } => self.check_variable_decl(
                name,
                type_annotation.as_ref(),
                initializer,
                &stmt.span,
                Mutability::Mutable,
            ),

            StmtKind::Const { name, initializer } => {
                let inferred = self.infer_expr(initializer)?;
                self.env.define(
                    name,
                    VarInfo {
                        ty: inferred,
                        mutability: Mutability::Constant,
                    },
                );
                Ok(())
            }

            StmtKind::Assignment { target, op, value } => self.check_assignment(target, op, value),

            StmtKind::ExprStmt(expr) => {
                self.infer_expr(expr)?;
                Ok(())
            }

            StmtKind::Block(stmts) => {
                self.env.push_scope();
                for s in stmts {
                    self.check_stmt(s)?;
                }
                self.env.pop_scope();
                Ok(())
            }

            StmtKind::If {
                condition,
                then_block,
                else_branch,
            } => self.check_if(condition, then_block, else_branch.as_ref()),

            StmtKind::While { condition, body } => self.check_while(condition, body),

            StmtKind::Return(value) => self.check_return(value.as_ref(), &stmt.span),

            StmtKind::Break | StmtKind::Continue => Ok(()),

            StmtKind::For { .. } => Err(TypeError::simple(
                "for-in loops are not supported yet".to_string(),
                stmt.span.clone(),
            )),

            StmtKind::When { subject, arms } => self.check_when(subject.as_ref(), arms, &stmt.span),

            StmtKind::LetDestructure { names, initializer } => {
                self.check_let_destructure(names, initializer, &stmt.span)
            }

            StmtKind::Func(func) => self.check_func_decl(func, &stmt.span),

            StmtKind::Class(decl) => self.check_class_decl(decl, &stmt.span),
            StmtKind::Trait(decl) => self.check_trait_decl(decl, &stmt.span),
            StmtKind::Enum(decl) => self.check_enum_decl(decl, &stmt.span),
            StmtKind::Struct(decl) => self.check_struct_decl(decl, &stmt.span),
            StmtKind::Import(import) => self.check_import(import, &stmt.span),
            StmtKind::WildcardImport(import) => self.check_wildcard_import(import, &stmt.span),
            StmtKind::Export(inner) => self.check_stmt(inner),

            StmtKind::Start(expr) => {
                self.infer_expr(expr)?;
                Ok(())
            }
        }
    }

    /// Infers the type of an expression.
    pub fn infer_expr(&mut self, expr: &Expr) -> Result<Type, TypeError> {
        match &expr.kind {
            ExprKind::Literal(lit) => Ok(infer_literal(lit)),

            ExprKind::Identifier(name) => self
                .env
                .lookup(name)
                .map(|info| info.ty.clone())
                .ok_or_else(|| {
                    let sugg =
                        suggestions::suggest_variable_weighted(name, &self.env, None, &expr.span);
                    TypeError::with_suggestions(
                        format!("undefined variable '{name}'"),
                        expr.span.clone(),
                        sugg,
                    )
                }),

            ExprKind::Binary { op, lhs, rhs } => self.infer_binary(op, lhs, rhs, &expr.span),

            ExprKind::Unary { op, operand } => self.infer_unary(op, operand),

            ExprKind::Grouped(inner) => self.infer_expr(inner),

            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => self.infer_ternary(condition, then_expr, else_expr, &expr.span),

            ExprKind::StringInterpolation(segments) => {
                for segment in segments {
                    if let InterpolationSegment::Expression(inner) = segment {
                        self.infer_expr(inner)?;
                    }
                }
                Ok(Type::Str)
            }

            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                let start_type = self.infer_expr(start)?;
                let end_type = self.infer_expr(end)?;
                // `..` between strings is concatenation
                if start_type == Type::Str && end_type == Type::Str {
                    if *inclusive {
                        return Err(TypeError::simple(
                            "string concatenation uses `..`, not `..=`".to_string(),
                            expr.span.clone(),
                        ));
                    }
                    Ok(Type::Str)
                } else if start_type.is_numeric() && end_type.is_numeric() {
                    // Numeric ranges (used in for..in and when patterns)
                    Ok(Type::Array(Box::new(start_type)))
                } else {
                    Err(TypeError::simple(
                        format!(
                            "`..` requires both operands to be strings (concatenation) or numeric (range), found {start_type} and {end_type}"
                        ),
                        expr.span.clone(),
                    ))
                }
            }

            ExprKind::NullCoalesce { lhs, rhs } => self.infer_null_coalesce(lhs, rhs, &expr.span),

            ExprKind::SafeAccess { object, member } => {
                let obj_type = self.infer_expr(object)?;
                match &obj_type {
                    Type::Optional(inner) => match inner.as_ref() {
                        Type::Class(_) | Type::Enum(_) => {
                            let member_type =
                                self.resolve_member_access(inner, member, &expr.span, false)?;
                            Ok(Type::Optional(Box::new(member_type)))
                        }
                        _ => Ok(Type::Optional(Box::new(Type::Unknown))),
                    },
                    _ => Err(TypeError::simple(
                        format!("'?.' requires Optional<T>, found {obj_type}"),
                        object.span.clone(),
                    )),
                }
            }

            ExprKind::MemberAccess { object, member } => {
                let is_self = matches!(&object.kind, ExprKind::Identifier(n) if n == "self");
                let obj_type = self.infer_expr(object)?;
                self.resolve_member_access(&obj_type, member, &expr.span, is_self)
            }

            ExprKind::Index { object, index } => {
                let obj_type = self.infer_expr(object)?;
                let _idx_type = self.infer_expr(index)?;
                match &obj_type {
                    Type::Array(inner) => Ok(*inner.clone()),
                    Type::Dictionary(_, v) => Ok(*v.clone()),
                    _ => Ok(Type::Unknown),
                }
            }

            ExprKind::Cast {
                expr: inner,
                target_type,
            } => {
                let source = self.infer_expr(inner)?;
                let target = self.resolve_type_expr(target_type, &expr.span)?;
                if source == target || (source.is_numeric() && target.is_numeric()) {
                    Ok(target)
                } else {
                    Err(TypeError::simple(
                        format!("cannot cast {source} to {target}"),
                        expr.span.clone(),
                    ))
                }
            }

            ExprKind::Call { callee, args } => self.infer_call(callee, args, &expr.span),

            ExprKind::Lambda { params, body } => self.infer_lambda(params, body, &expr.span),

            ExprKind::Tuple(elements) => {
                let types: Vec<Type> = elements
                    .iter()
                    .map(|e| self.infer_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Tuple(types))
            }

            ExprKind::ErrorPropagate(inner) => self.infer_error_propagate(inner, &expr.span),

            ExprKind::ArrayLiteral(elements) => self.infer_array_literal(elements, &expr.span),

            ExprKind::DictLiteral(entries) => self.infer_dict_literal(entries, &expr.span),

            ExprKind::NamespaceAccess { namespace, member } => {
                self.infer_namespace_access(namespace, member, &expr.span)
            }

            ExprKind::When { subject, arms } => {
                if let Some(subj) = subject {
                    self.infer_expr(subj)?;
                }
                let mut result_type: Option<Type> = None;
                for arm in arms {
                    let arm_type = match &arm.body {
                        WhenBody::Expr(e) => self.infer_expr(e)?,
                        WhenBody::Block(stmts) => {
                            // Type is the type of the last expression statement, or Void
                            if let Some(last) = stmts.last() {
                                if let StmtKind::ExprStmt(e) = &last.kind {
                                    self.infer_expr(e)?
                                } else {
                                    Type::Void
                                }
                            } else {
                                Type::Void
                            }
                        }
                    };
                    if let Some(ref expected) = result_type {
                        if *expected != arm_type
                            && arm_type != Type::Unknown
                            && *expected != Type::Unknown
                        {
                            return Err(TypeError::simple(
                                format!(
                                    "when expression arms must return the same type: expected {expected}, found {arm_type}"
                                ),
                                expr.span.clone(),
                            ));
                        }
                    } else {
                        result_type = Some(arm_type);
                    }
                }
                Ok(result_type.unwrap_or(Type::Void))
            }

            ExprKind::Yield(arg) => {
                if let Some(inner) = arg {
                    self.infer_expr(inner)?;
                }
                // Yield expressions produce the value pushed by the scheduler on resume.
                // For now, treat as Unknown (full coroutine type checking is future work).
                Ok(Type::Unknown)
            }

            ExprKind::Super { method, args } => self.infer_super(method, args, &expr.span),
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────

    /// Resolves a parser [`TypeExpr`] to a checked [`Type`].
    fn resolve_type_expr(&mut self, type_expr: &TypeExpr, span: &Span) -> Result<Type, TypeError> {
        match type_expr {
            TypeExpr::Simple(name) => match name.as_str() {
                "int" => Ok(Type::Int),
                "float" => Ok(Type::Float),
                "bool" => Ok(Type::Bool),
                "string" => Ok(Type::Str),
                "void" => Ok(Type::Void),
                other => {
                    if self.registry.get_class(other).is_some() {
                        Ok(Type::Class(other.to_string()))
                    } else if self.registry.get_trait(other).is_some() {
                        Ok(Type::Trait(other.to_string()))
                    } else if self.registry.get_enum(other).is_some() {
                        Ok(Type::Enum(other.to_string()))
                    } else if self.registry.get_struct(other).is_some() {
                        Ok(Type::Struct(other.to_string()))
                    } else {
                        Err(TypeError::with_suggestions(
                            format!("unknown type '{other}'"),
                            span.clone(),
                            suggestions::suggest_type_name(other, &self.registry, span),
                        ))
                    }
                }
            },
            TypeExpr::Generic { name, args } => match name.as_str() {
                "Result" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            "Result expects exactly one type argument".to_string(),
                            span.clone(),
                        ));
                    }
                    let inner = self.resolve_type_expr(&args[0], span)?;
                    Ok(Type::Result(Box::new(inner)))
                }
                "Optional" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            "Optional expects exactly one type argument".to_string(),
                            span.clone(),
                        ));
                    }
                    let inner = self.resolve_type_expr(&args[0], span)?;
                    Ok(Type::Optional(Box::new(inner)))
                }
                "Array" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            "Array expects exactly one type argument".to_string(),
                            span.clone(),
                        ));
                    }
                    let inner = self.resolve_type_expr(&args[0], span)?;
                    Ok(Type::Array(Box::new(inner)))
                }
                "Dictionary" => {
                    if args.len() != 2 {
                        return Err(TypeError::simple(
                            "Dictionary expects exactly two type arguments".to_string(),
                            span.clone(),
                        ));
                    }
                    let k = self.resolve_type_expr(&args[0], span)?;
                    let v = self.resolve_type_expr(&args[1], span)?;
                    Ok(Type::Dictionary(Box::new(k), Box::new(v)))
                }
                _ => {
                    // Try user-defined generic class or struct.
                    self.instantiate_generic(name, args, span)
                }
            },
            TypeExpr::Tuple(types) => {
                let resolved: Vec<Type> = types
                    .iter()
                    .map(|t| self.resolve_type_expr(t, span))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Type::Tuple(resolved))
            }
        }
    }

    /// Produces a synthetic monomorphic name for a generic instantiation.
    /// e.g. `Stack<int>` → `"Stack__int"`, `Pair<string, int>` → `"Pair__string__int"`.
    fn monomorphic_name(base: &str, arg_types: &[Type]) -> String {
        let mut name = base.to_string();
        for ty in arg_types {
            name.push_str("__");
            name.push_str(&ty.to_string());
        }
        name
    }

    /// Substitutes type-parameter references in a `TypeExpr` given a binding map.
    /// e.g. if bindings = {"T": "int"}, `Simple("T")` becomes `Simple("int")`.
    fn substitute_type_expr(type_expr: &TypeExpr, bindings: &HashMap<String, String>) -> TypeExpr {
        match type_expr {
            TypeExpr::Simple(name) => {
                if let Some(bound) = bindings.get(name) {
                    TypeExpr::Simple(bound.clone())
                } else {
                    TypeExpr::Simple(name.clone())
                }
            }
            TypeExpr::Generic { name, args } => TypeExpr::Generic {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|a| Self::substitute_type_expr(a, bindings))
                    .collect(),
            },
            TypeExpr::Tuple(types) => TypeExpr::Tuple(
                types
                    .iter()
                    .map(|t| Self::substitute_type_expr(t, bindings))
                    .collect(),
            ),
        }
    }

    /// Instantiates a user-defined generic class or struct for the given type arguments.
    /// Registers the concrete monomorphic type in the registry and returns its `Type`.
    fn instantiate_generic(
        &mut self,
        name: &str,
        args: &[TypeExpr],
        span: &Span,
    ) -> Result<Type, TypeError> {
        // Resolve the type arguments first.
        let mut arg_types = Vec::with_capacity(args.len());
        for arg in args {
            arg_types.push(self.resolve_type_expr(arg, span)?);
        }

        let mono_name = Self::monomorphic_name(name, &arg_types);

        // If already instantiated, return cached type.
        if self.registry.get_class(&mono_name).is_some() {
            return Ok(Type::Class(mono_name));
        }
        if self.registry.get_struct(&mono_name).is_some() {
            return Ok(Type::Struct(mono_name));
        }

        // Look up the template.
        if let Some(template) = self.generic_classes.get(name).cloned() {
            if template.type_params.len() != arg_types.len() {
                return Err(TypeError::simple(
                    format!(
                        "'{}' expects {} type argument(s), got {}",
                        name,
                        template.type_params.len(),
                        arg_types.len()
                    ),
                    span.clone(),
                ));
            }
            // Build substitution map: param name → concrete type display string.
            let bindings: HashMap<String, String> = template
                .type_params
                .iter()
                .zip(arg_types.iter())
                .map(|(p, t)| (p.clone(), t.to_string()))
                .collect();

            // Substitute fields.
            let mut fields = Vec::new();
            for field in &template.fields {
                let subst = Self::substitute_type_expr(&field.type_annotation, &bindings);
                let ty = self.resolve_type_expr(&subst, span)?;
                fields.push(FieldInfo {
                    name: field.name.clone(),
                    ty,
                    visibility: field.visibility,
                    has_default: field.default.is_some(),
                    has_setter: field.setter.is_some(),
                });
            }

            // Substitute methods.
            let mut methods = Vec::new();
            for method in &template.methods {
                let return_type = match &method.return_type {
                    Some(te) => {
                        let subst = Self::substitute_type_expr(te, &bindings);
                        self.resolve_type_expr(&subst, span)?
                    }
                    None => Type::Void,
                };
                let mut params = Vec::new();
                for param in &method.params {
                    let subst = Self::substitute_type_expr(&param.type_annotation, &bindings);
                    params.push(self.resolve_type_expr(&subst, span)?);
                }
                methods.push(MethodInfo {
                    name: method.name.clone(),
                    params,
                    return_type,
                    is_static: method.is_static,
                    visibility: method.visibility,
                    has_default_body: false,
                });
            }

            let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
            self.registry.register_class(ClassInfo {
                name: mono_name.clone(),
                fields,
                methods,
                parent: None,
                traits: Vec::new(),
            });
            self.env.define(
                &mono_name,
                VarInfo {
                    ty: Type::Function {
                        params: constructor_params,
                        return_type: Box::new(Type::Class(mono_name.clone())),
                    },
                    mutability: Mutability::Immutable,
                },
            );
            return Ok(Type::Class(mono_name));
        }

        if let Some(template) = self.generic_structs.get(name).cloned() {
            if template.type_params.len() != arg_types.len() {
                return Err(TypeError::simple(
                    format!(
                        "'{}' expects {} type argument(s), got {}",
                        name,
                        template.type_params.len(),
                        arg_types.len()
                    ),
                    span.clone(),
                ));
            }
            let bindings: HashMap<String, String> = template
                .type_params
                .iter()
                .zip(arg_types.iter())
                .map(|(p, t)| (p.clone(), t.to_string()))
                .collect();

            let mut fields = Vec::new();
            for field in &template.fields {
                let subst = Self::substitute_type_expr(&field.type_annotation, &bindings);
                let ty = self.resolve_type_expr(&subst, span)?;
                fields.push(FieldInfo {
                    name: field.name.clone(),
                    ty,
                    visibility: field.visibility,
                    has_default: field.default.is_some(),
                    has_setter: field.setter.is_some(),
                });
            }

            let mut methods = Vec::new();
            for method in &template.methods {
                let return_type = match &method.return_type {
                    Some(te) => {
                        let subst = Self::substitute_type_expr(te, &bindings);
                        self.resolve_type_expr(&subst, span)?
                    }
                    None => Type::Void,
                };
                let mut params = Vec::new();
                for param in &method.params {
                    let subst = Self::substitute_type_expr(&param.type_annotation, &bindings);
                    params.push(self.resolve_type_expr(&subst, span)?);
                }
                methods.push(MethodInfo {
                    name: method.name.clone(),
                    params,
                    return_type,
                    is_static: method.is_static,
                    visibility: method.visibility,
                    has_default_body: false,
                });
            }

            let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
            self.registry.register_struct(StructInfo {
                name: mono_name.clone(),
                fields,
                methods,
            });
            self.env.define(
                &mono_name,
                VarInfo {
                    ty: Type::Function {
                        params: constructor_params,
                        return_type: Box::new(Type::Struct(mono_name.clone())),
                    },
                    mutability: Mutability::Immutable,
                },
            );
            return Ok(Type::Struct(mono_name));
        }

        Err(TypeError::simple(
            format!("unknown generic type '{name}'"),
            span.clone(),
        ))
    }

    /// Checks if `actual` is compatible with `expected`.
    ///
    /// This allows `Optional<Unknown>` (from `null`) to match any `Optional<T>`,
    /// `Result<Unknown>` (from `Error(msg)`) to match any `Result<T>`,
    /// child classes to be assigned where parent is expected, and classes
    /// to be assigned where an implemented trait is expected.
    fn types_compatible(&self, expected: &Type, actual: &Type) -> bool {
        if expected == actual {
            return true;
        }
        match (expected, actual) {
            (Type::Optional(_), Type::Optional(inner)) if **inner == Type::Unknown => true,
            (Type::Result(_), Type::Result(inner)) if **inner == Type::Unknown => true,
            (Type::Class(parent), Type::Class(child)) => self.registry.is_subclass(child, parent),
            (Type::Trait(trait_name), Type::Class(class_name)) => self
                .registry
                .get_class(class_name)
                .map(|info| info.traits.contains(trait_name))
                .unwrap_or(false),
            // Function compatibility: param counts must match, each param and return
            // must be compatible. Unknown in expected position accepts any concrete type.
            (
                Type::Function {
                    params: expected_params,
                    return_type: expected_ret,
                },
                Type::Function {
                    params: actual_params,
                    return_type: actual_ret,
                },
            ) => {
                if expected_params.len() != actual_params.len() {
                    return false;
                }
                let params_ok = expected_params
                    .iter()
                    .zip(actual_params.iter())
                    .all(|(exp, act)| *exp == Type::Unknown || self.types_compatible(exp, act));
                let ret_ok = **expected_ret == Type::Unknown
                    || self.types_compatible(expected_ret, actual_ret);
                params_ok && ret_ok
            }
            // Empty array `[]` (Array<Unknown>) is assignable to any Array<T>.
            (Type::Array(_), Type::Array(inner)) if **inner == Type::Unknown => true,
            // Empty dict `{}` (Dictionary<Unknown, Unknown>) assignable to any Dictionary<K, V>.
            (Type::Dictionary(_, _), Type::Dictionary(k, v))
                if **k == Type::Unknown && **v == Type::Unknown =>
            {
                true
            }
            _ => false,
        }
    }

    fn check_variable_decl(
        &mut self,
        name: &str,
        type_annotation: Option<&TypeExpr>,
        initializer: &Expr,
        span: &Span,
        mutability: Mutability,
    ) -> Result<(), TypeError> {
        let inferred = self.infer_expr(initializer)?;

        let resolved = if let Some(annotation) = type_annotation {
            let annotated = self.resolve_type_expr(annotation, span)?;
            if !self.types_compatible(&annotated, &inferred) {
                return Err(TypeError::with_suggestions(
                    format!("type mismatch: expected {annotated}, found {inferred}"),
                    initializer.span.clone(),
                    suggestions::suggest_type_mismatch(&inferred, &annotated, &initializer.span),
                ));
            }
            annotated
        } else {
            inferred
        };

        self.env.define(
            name,
            VarInfo {
                ty: resolved,
                mutability,
            },
        );
        Ok(())
    }

    fn check_assignment(
        &mut self,
        target: &Expr,
        op: &AssignOp,
        value: &Expr,
    ) -> Result<(), TypeError> {
        // Index assignment: collection[index] = value
        if let ExprKind::Index { object, index } = &target.kind {
            self.infer_expr(object)?;
            self.infer_expr(index)?;
            self.infer_expr(value)?;
            return Ok(());
        }

        // Member assignment: obj.field = value
        if let ExprKind::MemberAccess { .. } = &target.kind {
            self.infer_expr(target)?;
            self.infer_expr(value)?;
            return Ok(());
        }

        let name = match &target.kind {
            ExprKind::Identifier(name) => name,
            _ => {
                return Err(TypeError::simple(
                    "assignment target must be a variable".to_string(),
                    target.span.clone(),
                ));
            }
        };

        let var_info = self.env.lookup(name).ok_or_else(|| {
            let sugg = suggestions::suggest_variable_weighted(name, &self.env, None, &target.span);
            TypeError::with_suggestions(
                format!("undefined variable '{name}'"),
                target.span.clone(),
                sugg,
            )
        })?;

        let target_type = var_info.ty.clone();
        let target_mutability = var_info.mutability;

        if target_mutability != Mutability::Mutable {
            let kind = match target_mutability {
                Mutability::Immutable => "immutable (declared with 'let')",
                Mutability::Constant => "a constant (declared with 'const')",
                Mutability::Mutable => unreachable!(),
            };
            return Err(TypeError::simple(
                format!("cannot assign to '{name}': variable is {kind}"),
                target.span.clone(),
            ));
        }

        let value_type = self.infer_expr(value)?;

        match op {
            AssignOp::Assign => {
                if target_type != value_type {
                    return Err(TypeError::with_suggestions(
                        format!("type mismatch: cannot assign {value_type} to {target_type}"),
                        value.span.clone(),
                        suggestions::suggest_type_mismatch(&value_type, &target_type, &value.span),
                    ));
                }
            }
            AssignOp::AddAssign
            | AssignOp::SubAssign
            | AssignOp::MulAssign
            | AssignOp::DivAssign
            | AssignOp::ModAssign => {
                if !target_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!(
                            "cannot use arithmetic assignment on non-numeric type {target_type}"
                        ),
                        target.span.clone(),
                    ));
                }
                if target_type != value_type {
                    return Err(TypeError::simple(
                        format!(
                            "type mismatch in compound assignment: expected {target_type}, found {value_type}"
                        ),
                        value.span.clone(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_if(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_branch: Option<&ElseBranch>,
    ) -> Result<(), TypeError> {
        let cond_type = self.infer_expr(condition)?;
        if cond_type != Type::Bool {
            return Err(TypeError::simple(
                format!("if condition must be bool, found {cond_type}"),
                condition.span.clone(),
            ));
        }

        self.env.push_scope();
        for stmt in then_block {
            self.check_stmt(stmt)?;
        }
        self.env.pop_scope();

        if let Some(branch) = else_branch {
            match branch {
                ElseBranch::ElseBlock(stmts) => {
                    self.env.push_scope();
                    for stmt in stmts {
                        self.check_stmt(stmt)?;
                    }
                    self.env.pop_scope();
                }
                ElseBranch::ElseIf(stmt) => {
                    self.check_stmt(stmt)?;
                }
            }
        }
        Ok(())
    }

    fn check_while(&mut self, condition: &Expr, body: &[Stmt]) -> Result<(), TypeError> {
        let cond_type = self.infer_expr(condition)?;
        if cond_type != Type::Bool {
            return Err(TypeError::simple(
                format!("while condition must be bool, found {cond_type}"),
                condition.span.clone(),
            ));
        }

        self.env.push_scope();
        for stmt in body {
            self.check_stmt(stmt)?;
        }
        self.env.pop_scope();
        Ok(())
    }

    fn infer_binary(
        &mut self,
        op: &BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let lhs_type = self.infer_expr(lhs)?;
        let rhs_type = self.infer_expr(rhs)?;

        match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo => {
                if !lhs_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!(
                            "left operand of arithmetic operator must be numeric, found {lhs_type}"
                        ),
                        lhs.span.clone(),
                    ));
                }
                if !rhs_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!(
                            "right operand of arithmetic operator must be numeric, found {rhs_type}"
                        ),
                        rhs.span.clone(),
                    ));
                }
                if lhs_type != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "arithmetic operands must be the same type: {lhs_type} vs {rhs_type}"
                        ),
                        span.clone(),
                    ));
                }
                Ok(lhs_type)
            }

            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::Greater
            | BinaryOp::LessEqual
            | BinaryOp::GreaterEqual => {
                if lhs_type != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "comparison operands must be the same type: {lhs_type} vs {rhs_type}"
                        ),
                        span.clone(),
                    ));
                }
                Ok(Type::Bool)
            }

            BinaryOp::And | BinaryOp::Or => {
                if lhs_type != Type::Bool {
                    return Err(TypeError::simple(
                        format!("left operand of logical operator must be bool, found {lhs_type}"),
                        lhs.span.clone(),
                    ));
                }
                if rhs_type != Type::Bool {
                    return Err(TypeError::simple(
                        format!("right operand of logical operator must be bool, found {rhs_type}"),
                        rhs.span.clone(),
                    ));
                }
                Ok(Type::Bool)
            }
        }
    }

    fn infer_unary(&mut self, op: &UnaryOp, operand: &Expr) -> Result<Type, TypeError> {
        let operand_type = self.infer_expr(operand)?;
        match op {
            UnaryOp::Negate => {
                if !operand_type.is_numeric() {
                    return Err(TypeError::simple(
                        format!("cannot negate non-numeric type {operand_type}"),
                        operand.span.clone(),
                    ));
                }
                Ok(operand_type)
            }
            UnaryOp::Not => {
                if operand_type != Type::Bool {
                    return Err(TypeError::simple(
                        format!("cannot apply '!' to non-bool type {operand_type}"),
                        operand.span.clone(),
                    ));
                }
                Ok(Type::Bool)
            }
        }
    }

    fn infer_ternary(
        &mut self,
        condition: &Expr,
        then_expr: &Expr,
        else_expr: &Expr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let cond_type = self.infer_expr(condition)?;
        if cond_type != Type::Bool {
            return Err(TypeError::simple(
                format!("ternary condition must be bool, found {cond_type}"),
                condition.span.clone(),
            ));
        }

        let then_type = self.infer_expr(then_expr)?;
        let else_type = self.infer_expr(else_expr)?;
        if then_type != else_type {
            return Err(TypeError::simple(
                format!("ternary branches must have the same type: {then_type} vs {else_type}"),
                span.clone(),
            ));
        }
        Ok(then_type)
    }

    // ── Phase 6: Functions + Return Types ────────────────────────────────

    fn check_func_decl(&mut self, func: &FuncDecl, span: &Span) -> Result<(), TypeError> {
        let return_type = match &func.return_type {
            Some(type_expr) => self.resolve_type_expr(type_expr, span)?,
            None => Type::Void,
        };

        let mut param_types = Vec::new();
        for param in &func.params {
            let ty = self.resolve_type_expr(&param.type_annotation, span)?;
            param_types.push(ty);
        }

        let func_type = Type::Function {
            params: param_types.clone(),
            return_type: Box::new(return_type.clone()),
        };
        self.env.define(
            &func.name,
            VarInfo {
                ty: func_type,
                mutability: Mutability::Immutable,
            },
        );

        let prev_return_type = self.current_return_type.take();
        self.current_return_type = Some(return_type.clone());

        self.env.push_scope();

        // Validate `where` clauses and bind type params to their constraint type in the body scope.
        // This lets the type checker resolve trait method calls on type parameters.
        for clause in &func.where_clauses {
            if self.registry.get_trait(&clause.trait_name).is_none() {
                return Err(TypeError::simple(
                    format!("unknown trait '{}' in where clause", clause.trait_name),
                    span.clone(),
                ));
            }
            // Bind the type param name to its constraint trait type so method calls resolve.
            self.env.define(
                &clause.type_param,
                VarInfo {
                    ty: Type::Trait(clause.trait_name.clone()),
                    mutability: Mutability::Immutable,
                },
            );
        }

        for (param, ty) in func.params.iter().zip(param_types.into_iter()) {
            self.env.define(
                &param.name,
                VarInfo {
                    ty,
                    mutability: Mutability::Immutable,
                },
            );
        }

        for stmt in &func.body {
            self.check_stmt(stmt)?;
        }

        if return_type != Type::Void && !returns_on_all_paths(&func.body) {
            self.env.pop_scope();
            self.current_return_type = prev_return_type;
            return Err(TypeError::simple(
                format!(
                    "missing return on some code paths in function '{}' returning {return_type}",
                    func.name
                ),
                span.clone(),
            ));
        }

        self.env.pop_scope();
        self.current_return_type = prev_return_type;
        Ok(())
    }

    fn check_return(&mut self, value: Option<&Expr>, span: &Span) -> Result<(), TypeError> {
        // At top level (outside any function), current_return_type is None.
        // Allow any return value there.
        let Some(expected) = self.current_return_type.clone() else {
            if let Some(expr) = value {
                self.infer_expr(expr)?;
            }
            return Ok(());
        };

        match value {
            Some(expr) => {
                let actual = self.infer_expr(expr)?;
                if expected == Type::Void {
                    return Err(TypeError::simple(
                        "cannot return a value from a void function".to_string(),
                        expr.span.clone(),
                    ));
                }
                if !self.types_compatible(&expected, &actual) {
                    return Err(TypeError::with_suggestions(
                        format!("return type mismatch: expected {expected}, found {actual}"),
                        expr.span.clone(),
                        suggestions::suggest_type_mismatch(&actual, &expected, &expr.span),
                    ));
                }
                Ok(())
            }
            None => {
                if expected != Type::Void {
                    return Err(TypeError::simple(
                        format!("missing return value: expected {expected}"),
                        span.clone(),
                    ));
                }
                Ok(())
            }
        }
    }

    fn infer_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        // Special-case Success() and Error() constructors for Result<T>.
        if let ExprKind::Identifier(name) = &callee.kind {
            match name.as_str() {
                "Success" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            format!("Success() expects 1 argument, found {}", args.len()),
                            span.clone(),
                        ));
                    }
                    let arg_expr = call_arg_expr(&args[0]);
                    let inner_type = self.infer_expr(arg_expr)?;
                    return Ok(Type::Result(Box::new(inner_type)));
                }
                "Error" => {
                    if args.len() != 1 {
                        return Err(TypeError::simple(
                            format!("Error() expects 1 argument, found {}", args.len()),
                            span.clone(),
                        ));
                    }
                    let arg_expr = call_arg_expr(&args[0]);
                    let arg_type = self.infer_expr(arg_expr)?;
                    if arg_type != Type::Str {
                        return Err(TypeError::simple(
                            format!("Error() expects a string message, found {arg_type}"),
                            arg_expr.span.clone(),
                        ));
                    }
                    return Ok(Type::Result(Box::new(Type::Unknown)));
                }
                _ => {}
            }
        }

        // Check if this is a constructor call for a registered class.
        if let ExprKind::Identifier(name) = &callee.kind
            && let Some(class_info) = self.registry.get_class(name).cloned()
        {
            return self.infer_constructor_call(&class_info, args, span);
        }

        // Check if this is a constructor call for a registered struct.
        if let ExprKind::Identifier(name) = &callee.kind
            && let Some(struct_info) = self.registry.get_struct(name).cloned()
        {
            return self.infer_struct_constructor_call(&struct_info, args, span);
        }

        let callee_type = self.infer_expr(callee)?;
        match callee_type {
            Type::Function {
                params,
                return_type,
            } => {
                // Untyped sentinel: a single Unknown param signals "accept any args".
                // Used by register_host_fn_untyped — still infers arg exprs for
                // undefined-variable detection but skips arity and type checks.
                if params.len() == 1 && params[0] == Type::Unknown {
                    for arg in args {
                        self.infer_expr(call_arg_expr(arg))?;
                    }
                    return Ok(*return_type);
                }
                if args.len() != params.len() {
                    return Err(TypeError::simple(
                        format!(
                            "expected {} argument(s), found {}",
                            params.len(),
                            args.len()
                        ),
                        span.clone(),
                    ));
                }
                for (i, (arg, expected_type)) in args.iter().zip(params.iter()).enumerate() {
                    let arg_expr = call_arg_expr(arg);
                    let actual = self.infer_expr(arg_expr)?;
                    if !self.types_compatible(expected_type, &actual) {
                        return Err(TypeError::simple(
                            format!(
                                "argument {} type mismatch: expected {expected_type}, found {actual}",
                                i + 1
                            ),
                            arg_expr.span.clone(),
                        ));
                    }
                }
                Ok(*return_type)
            }
            _ => Err(TypeError::simple(
                format!("type '{callee_type}' is not callable"),
                callee.span.clone(),
            )),
        }
    }

    fn infer_super(
        &mut self,
        method: &str,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        // Resolve the enclosing class and its parent.
        let class_name = self.current_class.clone().ok_or_else(|| {
            TypeError::simple("'super' used outside of a class method".to_string(), span.clone())
        })?;
        let parent_name = self
            .registry
            .get_class(&class_name)
            .and_then(|c| c.parent.clone())
            .ok_or_else(|| {
                TypeError::simple(
                    format!("'super' used in '{class_name}' which has no parent class"),
                    span.clone(),
                )
            })?;

        // Find the method on the parent class (searches the full parent chain).
        let method_info = self
            .registry
            .all_methods(&parent_name)
            .into_iter()
            .find(|m| m.name == method)
            .ok_or_else(|| {
                TypeError::simple(
                    format!("parent class '{parent_name}' has no method '{method}'"),
                    span.clone(),
                )
            })?;

        // Type-check arguments against the parent method's parameter types.
        // Skip the implicit `self` parameter (first param in the method info is self's type,
        // but MethodInfo.params only stores explicit params — consistent with infer_call).
        let params = &method_info.params;
        if args.len() != params.len() {
            return Err(TypeError::simple(
                format!(
                    "super.{method}() expects {} argument(s), found {}",
                    params.len(),
                    args.len()
                ),
                span.clone(),
            ));
        }
        for (i, (arg, expected)) in args.iter().zip(params.iter()).enumerate() {
            let arg_expr = call_arg_expr(arg);
            let actual = self.infer_expr(arg_expr)?;
            if !self.types_compatible(expected, &actual) {
                return Err(TypeError::simple(
                    format!(
                        "super.{method}() argument {} type mismatch: expected {expected}, found {actual}",
                        i + 1
                    ),
                    arg_expr.span.clone(),
                ));
            }
        }

        Ok(method_info.return_type.clone())
    }

    fn infer_error_propagate(&mut self, inner: &Expr, span: &Span) -> Result<Type, TypeError> {
        let inner_type = self.infer_expr(inner)?;
        let unwrapped = match &inner_type {
            Type::Result(t) => t.as_ref().clone(),
            _ => {
                return Err(TypeError::simple(
                    format!("'?' operator requires Result<T>, found {inner_type}"),
                    inner.span.clone(),
                ));
            }
        };
        match &self.current_return_type {
            Some(Type::Result(_)) => Ok(unwrapped),
            _ => Err(TypeError::with_suggestions(
                "'?' operator can only be used inside a function returning Result<T>".to_string(),
                span.clone(),
                vec![suggestions::Suggestion {
                    message: "the enclosing function must return Result<T> to use '?'".to_string(),
                    replacement: None,
                    span: span.clone(),
                }],
            )),
        }
    }

    fn infer_null_coalesce(
        &mut self,
        lhs: &Expr,
        rhs: &Expr,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let lhs_type = self.infer_expr(lhs)?;
        let rhs_type = self.infer_expr(rhs)?;

        match &lhs_type {
            Type::Optional(inner) => {
                if **inner != Type::Unknown && **inner != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "null coalescing type mismatch: Optional<{inner}> cannot fallback to {rhs_type}"
                        ),
                        rhs.span.clone(),
                    ));
                }
                Ok(rhs_type)
            }
            Type::Result(inner) => {
                if **inner != Type::Unknown && **inner != rhs_type {
                    return Err(TypeError::simple(
                        format!(
                            "null coalescing type mismatch: Result<{inner}> cannot fallback to {rhs_type}"
                        ),
                        rhs.span.clone(),
                    ));
                }
                Ok(rhs_type)
            }
            _ => Err(TypeError::simple(
                format!("'??' requires Optional<T> or Result<T> on the left, found {lhs_type}"),
                span.clone(),
            )),
        }
    }

    fn infer_lambda(
        &mut self,
        params: &[crate::parser::FuncParam],
        body: &LambdaBody,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let mut param_types = Vec::new();
        self.env.push_scope();

        for param in params {
            let ty = self.resolve_type_expr(&param.type_annotation, span)?;
            param_types.push(ty.clone());
            self.env.define(
                &param.name,
                VarInfo {
                    ty,
                    mutability: Mutability::Immutable,
                },
            );
        }

        let return_type = match body {
            LambdaBody::Expr(e) => self.infer_expr(e)?,
            LambdaBody::Block(stmts) => {
                for stmt in stmts {
                    self.check_stmt(stmt)?;
                }
                Type::Void
            }
        };

        self.env.pop_scope();

        Ok(Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        })
    }

    fn check_when(
        &mut self,
        subject: Option<&Expr>,
        arms: &[crate::parser::WhenArm],
        span: &Span,
    ) -> Result<(), TypeError> {
        let Some(subject_expr) = subject else {
            // Subject-less when — just check each arm body.
            for arm in arms {
                self.env.push_scope();
                self.check_when_body(&arm.body)?;
                self.env.pop_scope();
            }
            return Ok(());
        };

        let subject_type = self.infer_expr(subject_expr)?;

        match &subject_type {
            Type::Result(inner_type) => {
                let mut has_success = false;
                let mut has_error = false;
                let mut has_else = false;

                for arm in arms {
                    self.env.push_scope();
                    match &arm.pattern {
                        WhenPattern::TypeMatch { type_name, binding } => {
                            if type_name == "Success" {
                                has_success = true;
                                if let Some(name) = binding {
                                    self.env.define(
                                        name,
                                        VarInfo {
                                            ty: *inner_type.clone(),
                                            mutability: Mutability::Immutable,
                                        },
                                    );
                                }
                            } else if type_name == "Error" {
                                has_error = true;
                                if let Some(name) = binding {
                                    self.env.define(
                                        name,
                                        VarInfo {
                                            ty: Type::Str,
                                            mutability: Mutability::Immutable,
                                        },
                                    );
                                }
                            }
                        }
                        WhenPattern::Else => has_else = true,
                        _ => {}
                    }
                    self.check_when_body(&arm.body)?;
                    self.env.pop_scope();
                }

                if !(has_else || (has_success && has_error)) {
                    return Err(TypeError::simple("non-exhaustive when over Result<T>: must handle both 'is Success' and 'is Error' (or add 'else')"
                                .to_string(), span.clone()));
                }
                Ok(())
            }
            Type::Enum(enum_name) => {
                let enum_info = self.registry.get_enum(enum_name).ok_or(TypeError::simple(
                    format!("unknown enum '{enum_name}'"),
                    span.clone(),
                ))?;
                let all_variants: Vec<String> = enum_info.variants.clone();
                let mut covered: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut has_else = false;

                for arm in arms {
                    self.env.push_scope();
                    match &arm.pattern {
                        WhenPattern::Value(expr) => {
                            // Match bare identifier (variant name) or qualified EnumName.Variant.
                            let variant_name = match &expr.kind {
                                ExprKind::Identifier(name) => Some(name.clone()),
                                ExprKind::MemberAccess { member, .. } => Some(member.clone()),
                                _ => None,
                            };
                            if let Some(name) = variant_name
                                && all_variants.contains(&name)
                            {
                                covered.insert(name);
                            }
                        }
                        WhenPattern::Else => has_else = true,
                        _ => {}
                    }
                    self.check_when_body(&arm.body)?;
                    self.env.pop_scope();
                }

                if !has_else {
                    let missing: Vec<&str> = all_variants
                        .iter()
                        .filter(|v| !covered.contains(*v))
                        .map(|v| v.as_str())
                        .collect();
                    if !missing.is_empty() {
                        return Err(TypeError::simple(
                            format!(
                                "non-exhaustive when over {enum_name}: missing variant(s) {}",
                                missing.join(", ")
                            ),
                            span.clone(),
                        ));
                    }
                }
                Ok(())
            }
            _ => {
                // For non-Result, non-Enum subjects, just check each arm body.
                for arm in arms {
                    self.env.push_scope();
                    self.check_when_body(&arm.body)?;
                    self.env.pop_scope();
                }
                Ok(())
            }
        }
    }

    fn check_when_body(&mut self, body: &WhenBody) -> Result<(), TypeError> {
        match body {
            WhenBody::Expr(e) => {
                self.infer_expr(e)?;
                Ok(())
            }
            WhenBody::Block(stmts) => {
                for s in stmts {
                    self.check_stmt(s)?;
                }
                Ok(())
            }
        }
    }

    fn check_let_destructure(
        &mut self,
        names: &[String],
        initializer: &Expr,
        span: &Span,
    ) -> Result<(), TypeError> {
        let init_type = self.infer_expr(initializer)?;
        match init_type {
            Type::Tuple(types) => {
                if names.len() != types.len() {
                    return Err(TypeError::simple(
                        format!(
                            "tuple destructuring: expected {} names, found {}",
                            types.len(),
                            names.len()
                        ),
                        span.clone(),
                    ));
                }
                for (name, ty) in names.iter().zip(types.into_iter()) {
                    self.env.define(
                        name,
                        VarInfo {
                            ty,
                            mutability: Mutability::Immutable,
                        },
                    );
                }
                Ok(())
            }
            _ => Err(TypeError::simple(
                format!("cannot destructure non-tuple type {init_type}"),
                initializer.span.clone(),
            )),
        }
    }

    // ── Phase 7: Class, Trait, Enum checking (pass 2) ─────────────────

    fn check_trait_decl(&mut self, decl: &TraitDecl, span: &Span) -> Result<(), TypeError> {
        // Trait was already registered in pass 1. Now type-check default bodies.
        for method in &decl.methods {
            if let Some(body) = &method.default_body {
                self.env.push_scope();
                // Define `self` inside method scope.
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Trait(decl.name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                for param in &method.params {
                    let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                    self.env.define(
                        &param.name,
                        VarInfo {
                            ty,
                            mutability: Mutability::Immutable,
                        },
                    );
                }
                let return_type = match &method.return_type {
                    Some(te) => self.resolve_type_expr(te, span)?,
                    None => Type::Void,
                };
                let prev_return_type = self.current_return_type.take();
                self.current_return_type = Some(return_type.clone());
                for stmt in body {
                    self.check_stmt(stmt)?;
                }
                if return_type != Type::Void && !returns_on_all_paths(body) {
                    self.env.pop_scope();
                    self.current_return_type = prev_return_type;
                    return Err(TypeError::simple(
                        format!(
                            "missing return on some code paths in default method '{}' returning {return_type}",
                            method.name
                        ),
                        span.clone(),
                    ));
                }
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
            }
        }
        Ok(())
    }

    fn check_class_decl(&mut self, decl: &ClassDecl, span: &Span) -> Result<(), TypeError> {
        // Generic templates are checked only when instantiated.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let class_name = &decl.name;

        // Verify parent exists if extends is specified (checked in pass 2 to allow forward refs).
        if let Some(parent) = &decl.extends
            && self.registry.get_class(parent).is_none()
        {
            return Err(TypeError::simple(
                format!("unknown parent class '{parent}'"),
                span.clone(),
            ));
        }

        // Validate field defaults match their type annotations.
        for field in &decl.fields {
            if let Some(default_expr) = &field.default {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                let default_type = self.infer_expr(default_expr)?;
                if !self.types_compatible(&field_type, &default_type) {
                    return Err(TypeError::simple(
                        format!(
                            "field '{}' type mismatch: expected {field_type}, found {default_type}",
                            field.name
                        ),
                        default_expr.span.clone(),
                    ));
                }
            }
        }

        // Type-check methods.
        let prev_class = self.current_class.take();
        self.current_class = Some(class_name.clone());
        for method in &decl.methods {
            self.env.push_scope();

            // Define `self` inside method scope.
            if !method.is_static {
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Class(class_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );

                // Define all class fields as accessible variables (unqualified access
                // per spec section 6.3).
                let all_fields = self.registry.all_fields(class_name);
                for field in &all_fields {
                    self.env.define(
                        &field.name,
                        VarInfo {
                            ty: field.ty.clone(),
                            mutability: Mutability::Mutable,
                        },
                    );
                }
            }

            // Define method parameters.
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            for param in &method.params {
                let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                self.env.define(
                    &param.name,
                    VarInfo {
                        ty,
                        mutability: Mutability::Immutable,
                    },
                );
            }

            let prev_return_type = self.current_return_type.take();
            self.current_return_type = Some(return_type.clone());

            for stmt in &method.body {
                self.check_stmt(stmt)?;
            }

            if return_type != Type::Void && !returns_on_all_paths(&method.body) {
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
                return Err(TypeError::simple(
                    format!(
                        "missing return on some code paths in method '{}' returning {return_type}",
                        method.name
                    ),
                    span.clone(),
                ));
            }

            self.env.pop_scope();
            self.current_return_type = prev_return_type;
        }
        self.current_class = prev_class;

        // Validate trait implementations.
        self.validate_trait_implementations(decl, span)?;

        // Type-check setters.
        for field in &decl.fields {
            if let Some(setter) = &field.setter {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                self.env.push_scope();
                self.env.define(
                    &setter.param_name,
                    VarInfo {
                        ty: field_type.clone(),
                        mutability: Mutability::Immutable,
                    },
                );
                self.env.define(
                    "field",
                    VarInfo {
                        ty: field_type,
                        mutability: Mutability::Mutable,
                    },
                );
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Class(class_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                for stmt in &setter.body {
                    self.check_stmt(stmt)?;
                }
                self.env.pop_scope();
            }
        }

        Ok(())
    }

    fn validate_trait_implementations(
        &self,
        decl: &ClassDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        let class_info = self
            .registry
            .get_class(&decl.name)
            .expect("class should be registered in pass 1");

        // Collect all default method names from all traits (for conflict detection).
        let mut default_method_sources: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for trait_name in &decl.traits {
            let trait_info = match self.registry.get_trait(trait_name) {
                Some(info) => info,
                None => {
                    return Err(TypeError::simple(
                        format!("unknown trait '{trait_name}'"),
                        span.clone(),
                    ));
                }
            };

            for trait_method in &trait_info.methods {
                // Check if the class implements this method.
                let class_has_method = class_info
                    .methods
                    .iter()
                    .any(|m| m.name == trait_method.name);

                if !class_has_method && !trait_method.has_default_body {
                    return Err(TypeError::simple(
                        format!(
                            "class '{}' does not implement required method '{}' from trait '{trait_name}'",
                            decl.name, trait_method.name
                        ),
                        span.clone(),
                    ));
                }

                // Check signature match if the class provides an implementation.
                if class_has_method {
                    let class_method = class_info
                        .methods
                        .iter()
                        .find(|m| m.name == trait_method.name)
                        .unwrap();
                    if class_method.params != trait_method.params
                        || class_method.return_type != trait_method.return_type
                    {
                        return Err(TypeError::simple(
                            format!(
                                "method '{}' in class '{}' has a different signature than trait '{trait_name}' requires",
                                trait_method.name, decl.name
                            ),
                            span.clone(),
                        ));
                    }
                }

                // Track default method sources for conflict detection.
                if trait_method.has_default_body && !class_has_method {
                    default_method_sources
                        .entry(trait_method.name.clone())
                        .or_default()
                        .push(trait_name.clone());
                }
            }
        }

        // Detect conflicts: same default method name from multiple traits without class override.
        for (method_name, sources) in &default_method_sources {
            if sources.len() > 1 {
                return Err(TypeError::simple(
                    format!(
                        "conflicting default method '{method_name}' from traits {}; class '{}' must provide an override",
                        sources.join(" and "),
                        decl.name
                    ),
                    span.clone(),
                ));
            }
        }

        Ok(())
    }

    fn check_struct_decl(&mut self, decl: &StructDecl, span: &Span) -> Result<(), TypeError> {
        // Generic templates are checked only when instantiated.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let struct_name = &decl.name;

        // Validate field defaults match their type annotations.
        for field in &decl.fields {
            if let Some(default_expr) = &field.default {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                let default_type = self.infer_expr(default_expr)?;
                if !self.types_compatible(&field_type, &default_type) {
                    return Err(TypeError::simple(
                        format!(
                            "field '{}' type mismatch: expected {field_type}, found {default_type}",
                            field.name
                        ),
                        default_expr.span.clone(),
                    ));
                }
            }
        }

        // Type-check methods.
        for method in &decl.methods {
            self.env.push_scope();

            // Define `self` inside method scope.
            if !method.is_static {
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Struct(struct_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );

                // Define struct fields as accessible variables (unqualified access).
                let struct_info = self
                    .registry
                    .get_struct(struct_name)
                    .expect("struct should be registered in pass 1");
                for field in &struct_info.fields {
                    self.env.define(
                        &field.name,
                        VarInfo {
                            ty: field.ty.clone(),
                            mutability: Mutability::Mutable,
                        },
                    );
                }
            }

            // Define method parameters.
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            for param in &method.params {
                let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                self.env.define(
                    &param.name,
                    VarInfo {
                        ty,
                        mutability: Mutability::Immutable,
                    },
                );
            }

            let prev_return_type = self.current_return_type.take();
            self.current_return_type = Some(return_type.clone());

            for stmt in &method.body {
                self.check_stmt(stmt)?;
            }

            if return_type != Type::Void && !returns_on_all_paths(&method.body) {
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
                return Err(TypeError::simple(
                    format!(
                        "missing return on some code paths in method '{}' returning {return_type}",
                        method.name
                    ),
                    span.clone(),
                ));
            }

            self.env.pop_scope();
            self.current_return_type = prev_return_type;
        }

        // Type-check setters.
        for field in &decl.fields {
            if let Some(setter) = &field.setter {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                self.env.push_scope();
                self.env.define(
                    &setter.param_name,
                    VarInfo {
                        ty: field_type.clone(),
                        mutability: Mutability::Immutable,
                    },
                );
                self.env.define(
                    "field",
                    VarInfo {
                        ty: field_type,
                        mutability: Mutability::Mutable,
                    },
                );
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Struct(struct_name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                for stmt in &setter.body {
                    self.check_stmt(stmt)?;
                }
                self.env.pop_scope();
            }
        }

        Ok(())
    }

    fn check_enum_decl(&mut self, decl: &EnumDecl, span: &Span) -> Result<(), TypeError> {
        // Validate variant values if present.
        for variant in &decl.variants {
            if let Some(value_expr) = &variant.value {
                self.infer_expr(value_expr)?;
            }
        }

        // Type-check field defaults.
        for field in &decl.fields {
            if let Some(default_expr) = &field.default {
                let field_type = self.resolve_type_expr(&field.type_annotation, span)?;
                let default_type = self.infer_expr(default_expr)?;
                if !self.types_compatible(&field_type, &default_type) {
                    return Err(TypeError::simple(
                        format!(
                            "enum field '{}' type mismatch: expected {field_type}, found {default_type}",
                            field.name
                        ),
                        default_expr.span.clone(),
                    ));
                }
            }
        }

        // Type-check methods.
        for method in &decl.methods {
            self.env.push_scope();
            if !method.is_static {
                self.env.define(
                    "self",
                    VarInfo {
                        ty: Type::Enum(decl.name.clone()),
                        mutability: Mutability::Mutable,
                    },
                );
                // Define enum fields in method scope.
                let enum_info = self
                    .registry
                    .get_enum(&decl.name)
                    .expect("enum should be registered in pass 1");
                for field in &enum_info.fields {
                    self.env.define(
                        &field.name,
                        VarInfo {
                            ty: field.ty.clone(),
                            mutability: Mutability::Mutable,
                        },
                    );
                }
            }

            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            for param in &method.params {
                let ty = self.resolve_type_expr(&param.type_annotation, span)?;
                self.env.define(
                    &param.name,
                    VarInfo {
                        ty,
                        mutability: Mutability::Immutable,
                    },
                );
            }

            let prev_return_type = self.current_return_type.take();
            self.current_return_type = Some(return_type.clone());
            for stmt in &method.body {
                self.check_stmt(stmt)?;
            }
            if return_type != Type::Void && !returns_on_all_paths(&method.body) {
                self.env.pop_scope();
                self.current_return_type = prev_return_type;
                return Err(TypeError::simple(
                    format!(
                        "missing return on some code paths in enum method '{}' returning {return_type}",
                        method.name
                    ),
                    span.clone(),
                ));
            }
            self.env.pop_scope();
            self.current_return_type = prev_return_type;
        }

        Ok(())
    }

    fn infer_constructor_call(
        &mut self,
        class_info: &ClassInfo,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        let all_fields = self.registry.all_fields(&class_info.name);
        let required_fields: Vec<&FieldInfo> =
            all_fields.iter().filter(|f| !f.has_default).collect();

        // Check if args are named or positional.
        let has_named = args.iter().any(|a| matches!(a, CallArg::Named { .. }));

        if has_named {
            // Named args: match by field name.
            let mut provided: std::collections::HashSet<String> = std::collections::HashSet::new();
            for arg in args {
                match arg {
                    CallArg::Named { name, value } => {
                        let field = all_fields.iter().find(|f| f.name == *name);
                        match field {
                            Some(field_info) => {
                                let actual = self.infer_expr(value)?;
                                if !self.types_compatible(&field_info.ty, &actual) {
                                    return Err(TypeError::simple(
                                        format!(
                                            "constructor field '{name}' type mismatch: expected {}, found {actual}",
                                            field_info.ty
                                        ),
                                        value.span.clone(),
                                    ));
                                }
                                provided.insert(name.clone());
                            }
                            None => {
                                return Err(TypeError::simple(
                                    format!("class '{}' has no field '{name}'", class_info.name),
                                    value.span.clone(),
                                ));
                            }
                        }
                    }
                    CallArg::Positional(expr) => {
                        return Err(TypeError::simple(
                            "cannot mix positional and named arguments in constructor".to_string(),
                            expr.span.clone(),
                        ));
                    }
                }
            }
            // Check all required fields are provided.
            for field in &required_fields {
                if !provided.contains(&field.name) {
                    return Err(TypeError::simple(
                        format!(
                            "missing required field '{}' in constructor for '{}'",
                            field.name, class_info.name
                        ),
                        span.clone(),
                    ));
                }
            }
        } else {
            // Positional args: match in field order.
            // Must provide at least the required fields.
            if args.len() < required_fields.len() || args.len() > all_fields.len() {
                return Err(TypeError::simple(
                    format!(
                        "constructor for '{}' expects {}{} argument(s), found {}",
                        class_info.name,
                        if required_fields.len() == all_fields.len() {
                            String::new()
                        } else {
                            format!("{} to ", required_fields.len())
                        },
                        all_fields.len(),
                        args.len()
                    ),
                    span.clone(),
                ));
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_expr = call_arg_expr(arg);
                let actual = self.infer_expr(arg_expr)?;
                if !self.types_compatible(&all_fields[i].ty, &actual) {
                    return Err(TypeError::simple(
                        format!(
                            "constructor argument {} type mismatch: expected {}, found {actual}",
                            i + 1,
                            all_fields[i].ty
                        ),
                        arg_expr.span.clone(),
                    ));
                }
            }
        }

        Ok(Type::Class(class_info.name.clone()))
    }

    fn infer_struct_constructor_call(
        &mut self,
        struct_info: &StructInfo,
        args: &[CallArg],
        span: &Span,
    ) -> Result<Type, TypeError> {
        let all_fields = &struct_info.fields;
        let required_fields: Vec<&FieldInfo> =
            all_fields.iter().filter(|f| !f.has_default).collect();

        let has_named = args.iter().any(|a| matches!(a, CallArg::Named { .. }));

        if has_named {
            let mut provided: std::collections::HashSet<String> = std::collections::HashSet::new();
            for arg in args {
                match arg {
                    CallArg::Named { name, value } => {
                        let field = all_fields.iter().find(|f| f.name == *name);
                        match field {
                            Some(field_info) => {
                                let actual = self.infer_expr(value)?;
                                if !self.types_compatible(&field_info.ty, &actual) {
                                    return Err(TypeError::simple(
                                        format!(
                                            "constructor field '{name}' type mismatch: expected {}, found {actual}",
                                            field_info.ty
                                        ),
                                        value.span.clone(),
                                    ));
                                }
                                provided.insert(name.clone());
                            }
                            None => {
                                return Err(TypeError::simple(
                                    format!("struct '{}' has no field '{name}'", struct_info.name),
                                    value.span.clone(),
                                ));
                            }
                        }
                    }
                    CallArg::Positional(expr) => {
                        return Err(TypeError::simple(
                            "cannot mix positional and named arguments in constructor".to_string(),
                            expr.span.clone(),
                        ));
                    }
                }
            }
            for field in &required_fields {
                if !provided.contains(&field.name) {
                    return Err(TypeError::simple(
                        format!(
                            "missing required field '{}' in constructor for '{}'",
                            field.name, struct_info.name
                        ),
                        span.clone(),
                    ));
                }
            }
        } else {
            if args.len() < required_fields.len() || args.len() > all_fields.len() {
                return Err(TypeError::simple(
                    format!(
                        "constructor for '{}' expects {}{} argument(s), found {}",
                        struct_info.name,
                        if required_fields.len() == all_fields.len() {
                            String::new()
                        } else {
                            format!("{} to ", required_fields.len())
                        },
                        all_fields.len(),
                        args.len()
                    ),
                    span.clone(),
                ));
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_expr = call_arg_expr(arg);
                let actual = self.infer_expr(arg_expr)?;
                if !self.types_compatible(&all_fields[i].ty, &actual) {
                    return Err(TypeError::simple(
                        format!(
                            "constructor argument {} type mismatch: expected {}, found {actual}",
                            i + 1,
                            all_fields[i].ty
                        ),
                        arg_expr.span.clone(),
                    ));
                }
            }
        }

        Ok(Type::Struct(struct_info.name.clone()))
    }

    // ── Collection literal inference ────────────────────────────────────

    fn infer_array_literal(
        &mut self,
        elements: &[ArrayElement],
        span: &Span,
    ) -> Result<Type, TypeError> {
        if elements.is_empty() {
            return Ok(Type::Array(Box::new(Type::Unknown)));
        }

        let mut element_type: Option<Type> = None;
        for elem in elements {
            let ty = match elem {
                ArrayElement::Expr(e) => self.infer_expr(e)?,
                ArrayElement::Spread(e) => {
                    let spread_type = self.infer_expr(e)?;
                    match spread_type {
                        Type::Array(inner) => *inner,
                        other => {
                            return Err(TypeError::simple(
                                format!("spread requires Array<T>, found {other}"),
                                span.clone(),
                            ));
                        }
                    }
                }
            };

            match &element_type {
                None => element_type = Some(ty),
                Some(expected) => {
                    if !self.types_compatible(expected, &ty) {
                        return Err(TypeError::simple(
                            format!("array element type mismatch: expected {expected}, found {ty}"),
                            span.clone(),
                        ));
                    }
                }
            }
        }

        Ok(Type::Array(Box::new(element_type.unwrap())))
    }

    fn infer_dict_literal(
        &mut self,
        entries: &[DictElement],
        span: &Span,
    ) -> Result<Type, TypeError> {
        if entries.is_empty() {
            return Ok(Type::Dictionary(
                Box::new(Type::Unknown),
                Box::new(Type::Unknown),
            ));
        }

        let mut key_type: Option<Type> = None;
        let mut value_type: Option<Type> = None;
        for entry in entries {
            let (kt, vt) = match entry {
                DictElement::KeyValue { key, value } => {
                    (self.infer_expr(key)?, self.infer_expr(value)?)
                }
                DictElement::Spread(e) => {
                    let spread_type = self.infer_expr(e)?;
                    match spread_type {
                        Type::Dictionary(k, v) => (*k, *v),
                        other => {
                            return Err(TypeError::simple(
                                format!("spread requires Dictionary<K, V>, found {other}"),
                                span.clone(),
                            ));
                        }
                    }
                }
            };

            match &key_type {
                None => key_type = Some(kt),
                Some(expected) => {
                    if !self.types_compatible(expected, &kt) {
                        return Err(TypeError::simple(
                            format!(
                                "dictionary key type mismatch: expected {expected}, found {kt}"
                            ),
                            span.clone(),
                        ));
                    }
                }
            }
            match &value_type {
                None => value_type = Some(vt),
                Some(expected) => {
                    if !self.types_compatible(expected, &vt) {
                        return Err(TypeError::simple(
                            format!(
                                "dictionary value type mismatch: expected {expected}, found {vt}"
                            ),
                            span.clone(),
                        ));
                    }
                }
            }
        }

        Ok(Type::Dictionary(
            Box::new(key_type.unwrap()),
            Box::new(value_type.unwrap()),
        ))
    }

    fn infer_namespace_access(
        &self,
        namespace: &str,
        member: &str,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let module_path = self.namespace_aliases.get(namespace).ok_or_else(|| {
            TypeError::simple(format!("unknown namespace '{namespace}'"), span.clone())
        })?;

        self.module_registry
            .get_export(module_path, member)
            .cloned()
            .ok_or_else(|| {
                TypeError::simple(
                    format!("namespace '{namespace}' has no member '{member}'"),
                    span.clone(),
                )
            })
    }

    // ── Module resolution ─────────────────────────────────────────────

    fn check_import(&mut self, import: &ImportDecl, span: &Span) -> Result<(), TypeError> {
        let module = self
            .module_registry
            .get_module(&import.from)
            .ok_or_else(|| {
                TypeError::with_suggestions(
                    format!("unknown module path '{}'", import.from),
                    span.clone(),
                    suggestions::suggest_module_path(&import.from, &self.module_registry, span),
                )
            })?;

        // Clone the exports we need before mutating self.env.
        let mut resolved = Vec::new();
        for name in &import.names {
            let ty = module.get(name).ok_or_else(|| {
                TypeError::with_suggestions(
                    format!("module '{}' has no export '{name}'", import.from),
                    span.clone(),
                    suggestions::suggest_module_exports(&import.from, &self.module_registry, span),
                )
            })?;
            resolved.push((name.clone(), ty.clone()));
        }

        for (name, ty) in resolved {
            self.env.define(
                &name,
                VarInfo {
                    ty,
                    mutability: Mutability::Immutable,
                },
            );
        }
        Ok(())
    }

    fn check_wildcard_import(
        &mut self,
        import: &WildcardImportDecl,
        span: &Span,
    ) -> Result<(), TypeError> {
        if self.module_registry.get_module(&import.from).is_none() {
            return Err(TypeError::with_suggestions(
                format!("unknown module path '{}'", import.from),
                span.clone(),
                suggestions::suggest_module_path(&import.from, &self.module_registry, span),
            ));
        }

        self.namespace_aliases
            .insert(import.alias.clone(), import.from.clone());
        Ok(())
    }

    // ── Member access resolution ─────────────────────────────────────

    fn resolve_member_access(
        &self,
        obj_type: &Type,
        member: &str,
        span: &Span,
        is_internal: bool,
    ) -> Result<Type, TypeError> {
        match obj_type {
            Type::Class(class_name) => {
                // Search fields (including inherited).
                let all_fields = self.registry.all_fields(class_name);
                if let Some(field) = all_fields.iter().find(|f| f.name == member) {
                    if !is_internal && field.visibility != Visibility::Public {
                        return Err(TypeError::with_suggestions(
                            format!("field '{member}' of class '{class_name}' is private"),
                            span.clone(),
                            suggestions::suggest_public_getter(
                                member,
                                class_name,
                                &self.registry,
                                span,
                            ),
                        ));
                    }
                    return Ok(field.ty.clone());
                }

                // Search methods (including inherited and trait defaults).
                let all_methods = self.registry.all_methods(class_name);
                if let Some(method) = all_methods.iter().find(|m| m.name == member) {
                    if !is_internal && method.visibility != Visibility::Public {
                        return Err(TypeError::simple(
                            format!("method '{member}' of class '{class_name}' is private"),
                            span.clone(),
                        ));
                    }
                    return Ok(Type::Function {
                        params: method.params.clone(),
                        return_type: Box::new(method.return_type.clone()),
                    });
                }

                Err(TypeError::with_suggestions(
                    format!("type '{class_name}' has no member '{member}'"),
                    span.clone(),
                    suggestions::suggest_class_member(member, class_name, &self.registry, span),
                ))
            }
            Type::Enum(enum_name) => {
                let enum_info = match self.registry.get_enum(enum_name) {
                    Some(info) => info,
                    None => {
                        return Err(TypeError::simple(
                            format!("unknown enum '{enum_name}'"),
                            span.clone(),
                        ));
                    }
                };

                // Check variants.
                if enum_info.variants.iter().any(|v| v == member) {
                    return Ok(Type::Enum(enum_name.clone()));
                }

                // Check methods.
                if let Some(method) = enum_info.methods.iter().find(|m| m.name == member) {
                    return Ok(Type::Function {
                        params: method.params.clone(),
                        return_type: Box::new(method.return_type.clone()),
                    });
                }

                Err(TypeError::with_suggestions(
                    format!("enum '{enum_name}' has no member '{member}'"),
                    span.clone(),
                    suggestions::suggest_enum_member(member, enum_name, &self.registry, span),
                ))
            }
            Type::Struct(struct_name) => {
                let struct_info = match self.registry.get_struct(struct_name) {
                    Some(info) => info,
                    None => {
                        return Err(TypeError::simple(
                            format!("unknown struct '{struct_name}'"),
                            span.clone(),
                        ));
                    }
                };

                // Search fields.
                if let Some(field) = struct_info.fields.iter().find(|f| f.name == member) {
                    if !is_internal && field.visibility != Visibility::Public {
                        return Err(TypeError::simple(
                            format!("field '{member}' of struct '{struct_name}' is private"),
                            span.clone(),
                        ));
                    }
                    return Ok(field.ty.clone());
                }

                // Search methods.
                if let Some(method) = struct_info.methods.iter().find(|m| m.name == member) {
                    if !is_internal && method.visibility != Visibility::Public {
                        return Err(TypeError::simple(
                            format!("method '{member}' of struct '{struct_name}' is private"),
                            span.clone(),
                        ));
                    }
                    return Ok(Type::Function {
                        params: method.params.clone(),
                        return_type: Box::new(method.return_type.clone()),
                    });
                }

                Err(TypeError::with_suggestions(
                    format!("struct '{struct_name}' has no member '{member}'"),
                    span.clone(),
                    suggestions::suggest_struct_member(member, struct_name, &self.registry, span),
                ))
            }
            Type::Array(elem) => self.resolve_array_method(elem, member, span),
            Type::Dictionary(k, v) => self.resolve_dict_method(k, v, member, span),
            _ => Err(TypeError::simple(
                format!("cannot access member '{member}' on type '{obj_type}'"),
                span.clone(),
            )),
        }
    }

    fn resolve_array_method(
        &self,
        elem: &Type,
        method: &str,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let result = match method {
            "push" => Type::Function {
                params: vec![elem.clone()],
                return_type: Box::new(Type::Void),
            },
            "pop" => Type::Function {
                params: vec![],
                return_type: Box::new(elem.clone()),
            },
            "len" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Int),
            },
            "isEmpty" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Bool),
            },
            "contains" => Type::Function {
                params: vec![elem.clone()],
                return_type: Box::new(Type::Bool),
            },
            "indexOf" => Type::Function {
                params: vec![elem.clone()],
                return_type: Box::new(Type::Int),
            },
            "first" | "last" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Optional(Box::new(elem.clone()))),
            },
            "reverse" | "sort" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Void),
            },
            "map" => Type::Function {
                params: vec![Type::Function {
                    params: vec![elem.clone()],
                    return_type: Box::new(Type::Unknown),
                }],
                return_type: Box::new(Type::Array(Box::new(Type::Unknown))),
            },
            "filter" => Type::Function {
                params: vec![Type::Function {
                    params: vec![elem.clone()],
                    return_type: Box::new(Type::Bool),
                }],
                return_type: Box::new(Type::Array(Box::new(elem.clone()))),
            },
            "reduce" => Type::Function {
                params: vec![
                    Type::Function {
                        params: vec![Type::Unknown, elem.clone()],
                        return_type: Box::new(Type::Unknown),
                    },
                    Type::Unknown,
                ],
                return_type: Box::new(Type::Unknown),
            },
            "slice" => Type::Function {
                params: vec![Type::Int, Type::Int],
                return_type: Box::new(Type::Array(Box::new(elem.clone()))),
            },
            "insert" => Type::Function {
                params: vec![Type::Int, elem.clone()],
                return_type: Box::new(Type::Void),
            },
            "remove" => Type::Function {
                params: vec![Type::Int],
                return_type: Box::new(elem.clone()),
            },
            _ => {
                return Err(TypeError::simple(
                    format!("Array has no method '{method}'"),
                    span.clone(),
                ));
            }
        };
        Ok(result)
    }

    fn resolve_dict_method(
        &self,
        k: &Type,
        v: &Type,
        method: &str,
        span: &Span,
    ) -> Result<Type, TypeError> {
        let result = match method {
            "keys" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Array(Box::new(k.clone()))),
            },
            "values" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Array(Box::new(v.clone()))),
            },
            "contains" => Type::Function {
                params: vec![k.clone()],
                return_type: Box::new(Type::Bool),
            },
            "remove" => Type::Function {
                params: vec![k.clone()],
                return_type: Box::new(Type::Void),
            },
            "len" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Int),
            },
            "isEmpty" => Type::Function {
                params: vec![],
                return_type: Box::new(Type::Bool),
            },
            "merge" => Type::Function {
                params: vec![Type::Dictionary(Box::new(k.clone()), Box::new(v.clone()))],
                return_type: Box::new(Type::Void),
            },
            _ => {
                return Err(TypeError::simple(
                    format!("Dictionary has no method '{method}'"),
                    span.clone(),
                ));
            }
        };
        Ok(result)
    }

    // ── Phase 7: Type registration (pass 1) ─────────────────────────────

    /// Registers a type declaration (class, trait, or enum) in the registry
    /// without type-checking method bodies. Called during pass 1.
    fn register_type_if_decl(&mut self, stmt: &Stmt) -> Result<(), TypeError> {
        match &stmt.kind {
            StmtKind::Trait(decl) => self.register_trait(decl, &stmt.span),
            StmtKind::Class(decl) => self.register_class(decl, &stmt.span),
            StmtKind::Enum(decl) => self.register_enum(decl, &stmt.span),
            StmtKind::Struct(decl) => self.register_struct(decl, &stmt.span),
            StmtKind::Func(func) => {
                // Pre-register function signatures so they can be called from
                // class/trait/enum method bodies.
                self.register_func_signature(func, &stmt.span)
            }
            // Process imports in pass 1 so imported types are available for
            // class field annotations and other forward references.
            StmtKind::Import(import) => self.check_import(import, &stmt.span),
            StmtKind::WildcardImport(import) => self.check_wildcard_import(import, &stmt.span),
            // Unwrap exports and recurse to register the inner declaration.
            StmtKind::Export(inner) => self.register_type_if_decl(inner),
            _ => Ok(()),
        }
    }

    fn register_func_signature(&mut self, func: &FuncDecl, span: &Span) -> Result<(), TypeError> {
        let return_type = match &func.return_type {
            Some(type_expr) => self.resolve_type_expr(type_expr, span)?,
            None => Type::Void,
        };
        let mut param_types = Vec::new();
        for param in &func.params {
            let ty = self.resolve_type_expr(&param.type_annotation, span)?;
            param_types.push(ty);
        }
        let func_type = Type::Function {
            params: param_types,
            return_type: Box::new(return_type),
        };
        self.env.define(
            &func.name,
            VarInfo {
                ty: func_type,
                mutability: Mutability::Immutable,
            },
        );
        Ok(())
    }

    fn register_trait(&mut self, decl: &TraitDecl, span: &Span) -> Result<(), TypeError> {
        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: false,
                visibility: Visibility::Public,
                has_default_body: method.default_body.is_some(),
            });
        }
        self.registry.register_trait(TraitInfo {
            name: decl.name.clone(),
            methods,
        });
        Ok(())
    }

    fn register_class(&mut self, decl: &ClassDecl, span: &Span) -> Result<(), TypeError> {
        // Generic templates are not registered as concrete types — stored for later instantiation.
        if !decl.type_params.is_empty() {
            self.generic_classes.insert(decl.name.clone(), decl.clone());
            return Ok(());
        }

        let mut fields = Vec::new();
        for field in &decl.fields {
            let ty = self.resolve_type_expr(&field.type_annotation, span)?;
            fields.push(FieldInfo {
                name: field.name.clone(),
                ty,
                visibility: field.visibility,
                has_default: field.default.is_some(),
                has_setter: field.setter.is_some(),
            });
        }

        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: method.is_static,
                visibility: method.visibility,
                has_default_body: false,
            });
        }

        self.registry.register_class(ClassInfo {
            name: decl.name.clone(),
            fields: fields.clone(),
            methods,
            parent: decl.extends.clone(),
            traits: decl.traits.clone(),
        });

        // Register auto-generated constructor in the environment.
        let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
        self.env.define(
            &decl.name,
            VarInfo {
                ty: Type::Function {
                    params: constructor_params,
                    return_type: Box::new(Type::Class(decl.name.clone())),
                },
                mutability: Mutability::Immutable,
            },
        );

        Ok(())
    }

    fn register_struct(&mut self, decl: &StructDecl, span: &Span) -> Result<(), TypeError> {
        // Generic templates are not registered as concrete types — stored for later instantiation.
        if !decl.type_params.is_empty() {
            self.generic_structs.insert(decl.name.clone(), decl.clone());
            return Ok(());
        }

        let mut fields = Vec::new();
        for field in &decl.fields {
            let ty = self.resolve_type_expr(&field.type_annotation, span)?;
            fields.push(FieldInfo {
                name: field.name.clone(),
                ty,
                visibility: field.visibility,
                has_default: field.default.is_some(),
                has_setter: field.setter.is_some(),
            });
        }

        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: method.is_static,
                visibility: method.visibility,
                has_default_body: false,
            });
        }

        self.registry.register_struct(StructInfo {
            name: decl.name.clone(),
            fields: fields.clone(),
            methods,
        });

        // Register auto-generated constructor in the environment.
        let constructor_params: Vec<Type> = fields.iter().map(|f| f.ty.clone()).collect();
        self.env.define(
            &decl.name,
            VarInfo {
                ty: Type::Function {
                    params: constructor_params,
                    return_type: Box::new(Type::Struct(decl.name.clone())),
                },
                mutability: Mutability::Immutable,
            },
        );

        Ok(())
    }

    fn register_enum(&mut self, decl: &EnumDecl, span: &Span) -> Result<(), TypeError> {
        let variants: Vec<String> = decl.variants.iter().map(|v| v.name.clone()).collect();

        let mut fields = Vec::new();
        for field in &decl.fields {
            let ty = self.resolve_type_expr(&field.type_annotation, span)?;
            fields.push(FieldInfo {
                name: field.name.clone(),
                ty,
                visibility: field.visibility,
                has_default: field.default.is_some(),
                has_setter: field.setter.is_some(),
            });
        }

        let mut methods = Vec::new();
        for method in &decl.methods {
            let return_type = match &method.return_type {
                Some(te) => self.resolve_type_expr(te, span)?,
                None => Type::Void,
            };
            let mut params = Vec::new();
            for param in &method.params {
                params.push(self.resolve_type_expr(&param.type_annotation, span)?);
            }
            methods.push(MethodInfo {
                name: method.name.clone(),
                params,
                return_type,
                is_static: method.is_static,
                visibility: method.visibility,
                has_default_body: false,
            });
        }

        self.registry.register_enum(EnumInfo {
            name: decl.name.clone(),
            variants,
            fields,
            methods,
        });

        // Register enum name so Direction.North resolves via MemberAccess.
        self.env.define(
            &decl.name,
            VarInfo {
                ty: Type::Enum(decl.name.clone()),
                mutability: Mutability::Immutable,
            },
        );

        Ok(())
    }
}

fn infer_literal(lit: &Literal) -> Type {
    match lit {
        Literal::Int(_) => Type::Int,
        Literal::Float(_) => Type::Float,
        Literal::String(_) => Type::Str,
        Literal::Bool(_) => Type::Bool,
        Literal::Null => Type::Optional(Box::new(Type::Unknown)),
    }
}

/// Extracts the expression from a [`CallArg`], whether positional or named.
fn call_arg_expr(arg: &CallArg) -> &Expr {
    match arg {
        CallArg::Positional(expr) => expr,
        CallArg::Named { value, .. } => value,
    }
}

/// Returns `true` if every code path through `stmts` ends with a `return`.
fn returns_on_all_paths(stmts: &[Stmt]) -> bool {
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

        // Only one arg supplied to a two-param generic — should error.
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
}
