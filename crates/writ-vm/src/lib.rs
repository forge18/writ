//! Writ bytecode virtual machine — executes compiled bytecode instructions.

#[cfg(feature = "mobile-aosoa")]
mod aosoa;
pub(crate) mod class_instance;
mod coroutine;
mod debug;
mod error;
mod frame;
mod native;
mod object;
pub mod value;
mod vm;
mod writ_struct;

#[cfg(feature = "mobile-aosoa")]
pub use aosoa::AoSoAContainer;
pub use class_instance::WritClassInstance;
pub use coroutine::{Coroutine, CoroutineId, CoroutineState, WaitCondition};
pub use debug::{BreakpointAction, BreakpointContext};
pub use error::{RuntimeError, StackFrame, StackTrace};
pub use native::{NativeFunction, NativeMethod};
pub use object::WritObject;
pub use value::{ClosureData, Value, ValueTag};
pub use vm::VM;
pub use writ_struct::WritStruct;
