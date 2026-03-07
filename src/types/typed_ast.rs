use crate::parser::Stmt;

use super::types::Type;

/// A statement annotated with the checker-inferred type of its primary expression.
///
/// Produced by [`TypeChecker::check_program_typed`] and consumed by the bytecode
/// compiler. This thin wrapper threads type information across the checker/compiler
/// boundary so the compiler does not need to re-infer types independently.
///
/// The `expr_type` field carries:
/// - For `Let` / `Var` / `Const`: the inferred type of the initializer expression.
/// - For `ExprStmt`: the inferred type of the expression.
/// - For all other statements: [`Type::Void`].
#[derive(Debug, Clone)]
pub struct TypedStmt {
    pub stmt: Stmt,
    pub expr_type: Type,
}
