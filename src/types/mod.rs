//! Writ type checker — validates types in the AST before bytecode compilation.

mod checker;
mod env;
mod error;
mod module_registry;
mod registry;
pub mod suggestions;
mod typed_ast;
mod types;

pub use checker::TypeChecker;
pub use env::{Mutability, TypeEnv, VarInfo};
pub use error::TypeError;
pub use module_registry::ModuleRegistry;
pub use registry::{ClassInfo, EnumInfo, FieldInfo, MethodInfo, TraitInfo, TypeRegistry};
pub use suggestions::Suggestion;
pub use typed_ast::TypedStmt;
pub use types::Type;
