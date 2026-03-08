//! Writ bytecode compiler -- compiles AST to bytecode instructions.

mod chunk;
#[allow(clippy::module_inception)]
mod compiler;
mod error;
mod instruction;
mod local;
pub mod opcode;
mod peephole;
mod upvalue;

pub use chunk::Chunk;
pub use compiler::{ClassMeta, CompiledFunction, Compiler, StructMeta, string_hash};
pub use error::CompileError;
pub use instruction::{CmpOp, Instruction};
pub use local::Local;
pub use upvalue::UpvalueDescriptor;
