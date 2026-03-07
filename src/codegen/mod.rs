//! Writ code generation — produces Rust source from Writ type declarations.
//!
//! Generates struct definitions with `Deref`-based inheritance, trait
//! definitions, constructors, and `WritObject` implementations from the
//! type registry populated by `writ-types`.

mod rust_gen;
mod type_map;

pub use rust_gen::RustCodegen;
pub use type_map::{writ_type_to_rust, writ_type_to_rust_primitive};
