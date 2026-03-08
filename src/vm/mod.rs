//! Writ bytecode virtual machine — executes compiled bytecode instructions.

#[cfg(feature = "mobile-aosoa")]
mod aosoa;
pub mod binding;
pub(crate) mod class_instance;
mod coroutine;
mod debug;
mod error;
mod field_layout;
mod frame;
mod intern;
mod native;
mod object;
pub mod value;
mod vm;
mod writ_struct;

#[cfg(feature = "mobile-aosoa")]
pub use aosoa::AoSoAContainer;
pub use binding::{
    Fn0, Fn1, Fn2, Fn3, FromValue, IntoNativeHandler, IntoNativeMethodHandler, IntoValue, MFn0,
    MFn1, MFn2, MFn3, fn0, fn1, fn2, fn3, mfn0, mfn1, mfn2, mfn3,
};
pub use class_instance::WritClassInstance;
pub use coroutine::{Coroutine, CoroutineId, CoroutineState, WaitCondition};
#[cfg(feature = "debug-hooks")]
pub use debug::{BreakpointAction, BreakpointContext};
pub use error::{RuntimeError, StackFrame, StackTrace};
pub use field_layout::FieldLayout;
pub use intern::StringInterner;
pub use native::{NativeFn, NativeFunction, NativeMethod};
pub use object::WritObject;
pub use value::{ClosureData, Value, ValueTag};
pub use vm::VM;
pub use writ_struct::WritStruct;
