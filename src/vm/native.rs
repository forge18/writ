use std::rc::Rc;

use super::binding::{IntoNativeHandler, IntoNativeMethodHandler};
use super::sequence::NativeResult;
use super::value::Value;

/// Type alias for native function callables.
///
/// Native functions receive a slice of [`Value`] arguments and return
/// either a [`Value`] result or a `String` error message.
pub type NativeFn = Rc<dyn Fn(&[Value]) -> Result<Value, String>>;

/// Type alias for native method callables.
///
/// Native methods receive the receiver value and a slice of arguments,
/// and return either a [`Value`] result or a `String` error message.
pub type NativeMethodFn = Rc<dyn Fn(&Value, &[Value]) -> Result<Value, String>>;

/// A host-registered native function callable from Writ scripts.
pub struct NativeFunction {
    /// The function name as seen by scripts.
    pub name: String,
    /// Optional module this function belongs to (for `disable_module` filtering).
    pub module: Option<String>,
    /// Expected argument count. `None` means variadic.
    pub arity: Option<u8>,
    /// The callable body.
    pub body: NativeFn,
}

impl NativeFunction {
    /// Constructs a `NativeFunction` from a typed handler.
    /// Arity is inferred from the handler's type.
    pub fn from_handler<H: IntoNativeHandler>(
        name: &str,
        module: Option<&str>,
        handler: H,
    ) -> Self {
        NativeFunction {
            name: name.to_string(),
            module: module.map(|m| m.to_string()),
            arity: H::arity(),
            body: handler.into_handler(),
        }
    }
}

/// A host-registered method callable on a specific value type.
pub struct NativeMethod {
    /// The method name as seen by scripts.
    pub name: String,
    /// Optional module this method belongs to (for `disable_module` filtering).
    pub module: Option<String>,
    /// Expected argument count (not including the receiver). `None` means variadic.
    pub arity: Option<u8>,
    /// The callable body. Receives the receiver value and arguments.
    pub body: NativeMethodFn,
}

impl NativeMethod {
    /// Constructs a `NativeMethod` from a typed handler.
    /// Arity is inferred from the handler's type.
    pub fn from_handler<H: IntoNativeMethodHandler>(
        name: &str,
        module: Option<&str>,
        handler: H,
    ) -> Self {
        NativeMethod {
            name: name.to_string(),
            module: module.map(|m| m.to_string()),
            arity: H::arity(),
            body: handler.into_method_handler(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sequence-capable native types (callback support)
// ---------------------------------------------------------------------------

/// Native function that may return a [`NativeResult::Sequence`] for deferred
/// callback invocation.
pub type NativeSeqFn = Rc<dyn Fn(&[Value]) -> Result<NativeResult, String>>;

/// Native method that may return a [`NativeResult::Sequence`].
pub type NativeSeqMethodFn = Rc<dyn Fn(&Value, &[Value]) -> Result<NativeResult, String>>;

/// A host-registered native function that may invoke script callbacks via sequences.
pub struct SeqNativeFunction {
    pub module: Option<String>,
    pub arity: Option<u8>,
    pub body: NativeSeqFn,
}

/// A host-registered method that may invoke script callbacks via sequences.
pub struct SeqNativeMethod {
    pub name: String,
    pub module: Option<String>,
    pub arity: Option<u8>,
    pub body: NativeSeqMethodFn,
}
