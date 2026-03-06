use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::native::{NativeFn, NativeMethodFn};
use crate::object::WritObject;
use crate::value::Value;

// ── FromValue ────────────────────────────────────────────────────────────────

/// Extracts a concrete Rust type from a [`Value`] at call-site position `pos`.
///
/// Width coercion follows the spec:
/// - Widening (e.g. `I32 → i64`) is free and always succeeds.
/// - Narrowing (e.g. `I64 → i32`) performs a range check and returns `Err`
///   on overflow.
pub trait FromValue: Sized {
    fn from_value(v: &Value, pos: usize) -> Result<Self, String>;
}

impl FromValue for i32 {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::I32(n) => Ok(*n),
            Value::I64(n) => {
                i32::try_from(*n).map_err(|_| format!("arg {pos}: i64 value {n} overflows i32"))
            }
            other => Err(format!(
                "arg {pos}: expected int, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for i64 {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::I32(n) => Ok(*n as i64),
            Value::I64(n) => Ok(*n),
            other => Err(format!(
                "arg {pos}: expected int, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for f32 {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::F32(f) => Ok(*f),
            Value::F64(f) => Ok(*f as f32),
            other => Err(format!(
                "arg {pos}: expected float, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for f64 {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::F32(f) => Ok(*f as f64),
            Value::F64(f) => Ok(*f),
            other => Err(format!(
                "arg {pos}: expected float, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for bool {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::Bool(b) => Ok(*b),
            other => Err(format!(
                "arg {pos}: expected bool, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for String {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::Str(s) => Ok((**s).clone()),
            other => Err(format!(
                "arg {pos}: expected string, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for Rc<String> {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::Str(s) => Ok(Rc::clone(s)),
            other => Err(format!(
                "arg {pos}: expected string, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for Value {
    #[inline]
    fn from_value(v: &Value, _pos: usize) -> Result<Self, String> {
        Ok(v.cheap_clone())
    }
}

impl FromValue for Option<Value> {
    #[inline]
    fn from_value(v: &Value, _pos: usize) -> Result<Self, String> {
        Ok(Some(v.cheap_clone()))
    }
}

impl FromValue for Rc<RefCell<dyn WritObject>> {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::Object(obj) => Ok(Rc::clone(obj)),
            other => Err(format!(
                "arg {pos}: expected object, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for Rc<RefCell<Vec<Value>>> {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::Array(a) => Ok(Rc::clone(a)),
            other => Err(format!(
                "arg {pos}: expected array, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for Rc<RefCell<HashMap<String, Value>>> {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::Dict(d) => Ok(Rc::clone(d)),
            other => Err(format!(
                "arg {pos}: expected dictionary, got {}",
                other.type_name()
            )),
        }
    }
}

// ── IntoValue ────────────────────────────────────────────────────────────────

/// Wraps a concrete Rust type back into a [`Value`] after a native call returns.
pub trait IntoValue {
    fn into_value(self) -> Value;
}

impl IntoValue for Value {
    #[inline]
    fn into_value(self) -> Value {
        self
    }
}

impl IntoValue for () {
    #[inline]
    fn into_value(self) -> Value {
        Value::Null
    }
}

impl IntoValue for bool {
    #[inline]
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

impl IntoValue for i32 {
    #[inline]
    fn into_value(self) -> Value {
        Value::I32(self)
    }
}

impl IntoValue for i64 {
    #[inline]
    fn into_value(self) -> Value {
        Value::I64(self)
    }
}

impl IntoValue for f32 {
    #[inline]
    fn into_value(self) -> Value {
        Value::F32(self)
    }
}

impl IntoValue for f64 {
    #[inline]
    fn into_value(self) -> Value {
        Value::F64(self)
    }
}

impl IntoValue for String {
    #[inline]
    fn into_value(self) -> Value {
        Value::Str(Rc::new(self))
    }
}

impl IntoValue for Rc<String> {
    #[inline]
    fn into_value(self) -> Value {
        Value::Str(self)
    }
}

impl<T: IntoValue> IntoValue for Option<T> {
    #[inline]
    fn into_value(self) -> Value {
        self.map(|v| v.into_value()).unwrap_or(Value::Null)
    }
}

// ── IntoNativeHandler ────────────────────────────────────────────────────────

/// Converts a typed Rust function into a type-erased [`NativeFn`].
///
/// Each arity uses a distinct wrapper struct carrying argument types as phantom
/// type parameters, avoiding conflicting blanket impls. All typed functions
/// must return `Result<R, String>` where `R: IntoValue`.
pub trait IntoNativeHandler {
    fn arity() -> Option<u8>;
    fn into_handler(self) -> NativeFn;
}

// Wrapper structs — one per arity, with phantom type params for arg types.
// Constructor functions (fn0, fn1, ...) fill in PhantomData automatically so
// call sites can write `fn1(|x: f64| ...)` without spelling out type args.
pub struct Fn0<F>(pub F);
pub struct Fn1<A0, F>(pub F, pub std::marker::PhantomData<fn(A0)>);
pub struct Fn2<A0, A1, F>(pub F, pub std::marker::PhantomData<fn(A0, A1)>);
pub struct Fn3<A0, A1, A2, F>(pub F, pub std::marker::PhantomData<fn(A0, A1, A2)>);

/// Wrap a 0-argument typed function.
#[inline]
pub fn fn0<F>(f: F) -> Fn0<F> {
    Fn0(f)
}
/// Wrap a 1-argument typed function. Type params inferred from closure signature.
#[inline]
pub fn fn1<A0, F>(f: F) -> Fn1<A0, F> {
    Fn1(f, std::marker::PhantomData)
}
/// Wrap a 2-argument typed function.
#[inline]
pub fn fn2<A0, A1, F>(f: F) -> Fn2<A0, A1, F> {
    Fn2(f, std::marker::PhantomData)
}
/// Wrap a 3-argument typed function.
#[inline]
pub fn fn3<A0, A1, A2, F>(f: F) -> Fn3<A0, A1, A2, F> {
    Fn3(f, std::marker::PhantomData)
}

impl<R, F> IntoNativeHandler for Fn0<F>
where
    R: IntoValue + 'static,
    F: Fn() -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(0)
    }
    fn into_handler(self) -> NativeFn {
        Rc::new(move |_args: &[Value]| Ok(self.0()?.into_value()))
    }
}

impl<A0, R, F> IntoNativeHandler for Fn1<A0, F>
where
    A0: FromValue + 'static,
    R: IntoValue + 'static,
    F: Fn(A0) -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(1)
    }
    fn into_handler(self) -> NativeFn {
        Rc::new(move |args: &[Value]| {
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 0)?;
            Ok(self.0(a0)?.into_value())
        })
    }
}

impl<A0, A1, R, F> IntoNativeHandler for Fn2<A0, A1, F>
where
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    R: IntoValue + 'static,
    F: Fn(A0, A1) -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(2)
    }
    fn into_handler(self) -> NativeFn {
        Rc::new(move |args: &[Value]| {
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 0)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 1)?;
            Ok(self.0(a0, a1)?.into_value())
        })
    }
}

impl<A0, A1, A2, R, F> IntoNativeHandler for Fn3<A0, A1, A2, F>
where
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    A2: FromValue + 'static,
    R: IntoValue + 'static,
    F: Fn(A0, A1, A2) -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(3)
    }
    fn into_handler(self) -> NativeFn {
        Rc::new(move |args: &[Value]| {
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 0)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 1)?;
            let a2 = A2::from_value(args.get(2).ok_or("missing arg 2")?, 2)?;
            Ok(self.0(a0, a1, a2)?.into_value())
        })
    }
}

// ── IntoNativeMethodHandler ──────────────────────────────────────────────────

/// Converts a typed Rust method into a type-erased [`NativeMethodFn`].
///
/// The receiver is the first argument, typed via `FromValue`. Uses the same
/// wrapper-struct-per-arity pattern as `IntoNativeHandler`.
pub trait IntoNativeMethodHandler {
    fn arity() -> Option<u8>;
    fn into_method_handler(self) -> NativeMethodFn;
}

// Method wrappers — receiver is the first arg, typed via FromValue.
type Ph<T> = std::marker::PhantomData<T>;
pub struct MFn0<Recv, F>(pub F, pub Ph<fn(Recv)>);
pub struct MFn1<Recv, A0, F>(pub F, pub Ph<fn(Recv, A0)>);
pub struct MFn2<Recv, A0, A1, F>(pub F, pub Ph<fn(Recv, A0, A1)>);
pub struct MFn3<Recv, A0, A1, A2, F>(pub F, pub Ph<fn(Recv, A0, A1, A2)>);

/// Wrap a 0-arg method (receiver only). Type params inferred from closure.
#[inline]
pub fn mfn0<Recv, F>(f: F) -> MFn0<Recv, F> {
    MFn0(f, std::marker::PhantomData)
}
/// Wrap a 1-arg method.
#[inline]
pub fn mfn1<Recv, A0, F>(f: F) -> MFn1<Recv, A0, F> {
    MFn1(f, std::marker::PhantomData)
}
/// Wrap a 2-arg method.
#[inline]
pub fn mfn2<Recv, A0, A1, F>(f: F) -> MFn2<Recv, A0, A1, F> {
    MFn2(f, std::marker::PhantomData)
}
/// Wrap a 3-arg method.
#[inline]
pub fn mfn3<Recv, A0, A1, A2, F>(f: F) -> MFn3<Recv, A0, A1, A2, F> {
    MFn3(f, std::marker::PhantomData)
}

impl<Recv, R, F> IntoNativeMethodHandler for MFn0<Recv, F>
where
    Recv: FromValue + 'static,
    R: IntoValue + 'static,
    F: Fn(Recv) -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(0)
    }
    fn into_method_handler(self) -> NativeMethodFn {
        Rc::new(move |receiver: &Value, _args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            Ok(self.0(recv)?.into_value())
        })
    }
}

impl<Recv, A0, R, F> IntoNativeMethodHandler for MFn1<Recv, A0, F>
where
    Recv: FromValue + 'static,
    A0: FromValue + 'static,
    R: IntoValue + 'static,
    F: Fn(Recv, A0) -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(1)
    }
    fn into_method_handler(self) -> NativeMethodFn {
        Rc::new(move |receiver: &Value, args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 1)?;
            Ok(self.0(recv, a0)?.into_value())
        })
    }
}

impl<Recv, A0, A1, R, F> IntoNativeMethodHandler for MFn2<Recv, A0, A1, F>
where
    Recv: FromValue + 'static,
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    R: IntoValue + 'static,
    F: Fn(Recv, A0, A1) -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(2)
    }
    fn into_method_handler(self) -> NativeMethodFn {
        Rc::new(move |receiver: &Value, args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 1)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 2)?;
            Ok(self.0(recv, a0, a1)?.into_value())
        })
    }
}

impl<Recv, A0, A1, A2, R, F> IntoNativeMethodHandler for MFn3<Recv, A0, A1, A2, F>
where
    Recv: FromValue + 'static,
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    A2: FromValue + 'static,
    R: IntoValue + 'static,
    F: Fn(Recv, A0, A1, A2) -> Result<R, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(3)
    }
    fn into_method_handler(self) -> NativeMethodFn {
        Rc::new(move |receiver: &Value, args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 1)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 2)?;
            let a2 = A2::from_value(args.get(2).ok_or("missing arg 2")?, 3)?;
            Ok(self.0(recv, a0, a1, a2)?.into_value())
        })
    }
}
