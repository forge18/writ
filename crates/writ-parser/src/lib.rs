//! Writ parser — parses token streams into an AST of expressions and statements.

mod ast;
mod error;
mod parser;

pub use ast::{
    ArrayElement, AssignOp, BinaryOp, CallArg, ClassDecl, Decl, DeclKind, DictElement, ElseBranch,
    EnumDecl, EnumVariant, Expr, ExprKind, FieldDecl, FuncDecl, FuncParam, ImportDecl,
    InterpolationSegment, LambdaBody, Literal, Setter, Stmt, StmtKind, StructDecl, TraitDecl,
    TraitMethod, TypeExpr, UnaryOp, Visibility, WhenArm, WhenBody, WhenPattern, WildcardImportDecl,
};
pub use error::ParseError;
pub use parser::Parser;
