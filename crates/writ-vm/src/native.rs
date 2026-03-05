use std::rc::Rc;

use crate::value::Value;

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
