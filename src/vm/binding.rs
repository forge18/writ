use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::native::{NativeFn, NativeMethodFn, NativeSeqFn, NativeSeqMethodFn};
use super::object::WritObject;
use super::sequence::{NativeResult, WritFn};
use super::value::Value;

/// Extracts a concrete Rust type from a [`Value`] at call-site position `pos`.
///
/// Width coercion follows the spec:
/// - Widening (e.g. `I32 -> i64`) is free and always succeeds.
/// - Narrowing (e.g. `I64 -> i32`) performs a range check and returns `Err`
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
            Value::Str(s) => Ok(s.to_string()),
            other => Err(format!(
                "arg {pos}: expected string, got {}",
                other.type_name()
            )),
        }
    }
}

impl FromValue for Rc<str> {
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

impl FromValue for WritFn {
    #[inline]
    fn from_value(v: &Value, pos: usize) -> Result<Self, String> {
        match v {
            Value::Closure(_) | Value::Str(_) => Ok(WritFn::new(v.cheap_clone())),
            other => Err(format!(
                "arg {pos}: expected function or closure, got {}",
                other.type_name()
            )),
        }
    }
}

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
        Value::Str(Rc::from(self.as_str()))
    }
}

impl IntoValue for Rc<str> {
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

/// Converts a typed Rust function into a type-erased [`NativeFn`].
///
/// Each arity uses a distinct wrapper struct carrying argument types as phantom
/// type parameters, avoiding conflicting blanket impls. All typed functions
/// must return `Result<R, String>` where `R: IntoValue`.
pub trait IntoNativeHandler {
    fn arity() -> Option<u8>;
    fn into_handler(self) -> NativeFn;
}

// Wrapper structs -- one per arity, with phantom type params for arg types.
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

/// Converts a typed Rust method into a type-erased [`NativeMethodFn`].
///
/// The receiver is the first argument, typed via `FromValue`. Uses the same
/// wrapper-struct-per-arity pattern as `IntoNativeHandler`.
pub trait IntoNativeMethodHandler {
    fn arity() -> Option<u8>;
    fn into_method_handler(self) -> NativeMethodFn;
}

// Method wrappers -- receiver is the first arg, typed via FromValue.
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

// ===========================================================================
// Sequence-capable handlers (callback support via trampoline)
// ===========================================================================

/// Converts a typed Rust function into a [`NativeSeqFn`] that may return
/// a [`NativeResult::Sequence`] for deferred callback invocation.
pub trait IntoNativeSeqHandler {
    fn arity() -> Option<u8>;
    fn into_seq_handler(self) -> NativeSeqFn;
}

// Wrapper structs for seq functions (parallel to Fn0-Fn3).
pub struct SeqFn1<A0, F>(pub F, pub std::marker::PhantomData<fn(A0)>);
pub struct SeqFn2<A0, A1, F>(pub F, pub std::marker::PhantomData<fn(A0, A1)>);
pub struct SeqFn3<A0, A1, A2, F>(pub F, pub std::marker::PhantomData<fn(A0, A1, A2)>);

/// Wrap a 1-argument sequence-capable function.
#[inline]
pub fn seq_fn1<A0, F>(f: F) -> SeqFn1<A0, F> {
    SeqFn1(f, std::marker::PhantomData)
}
/// Wrap a 2-argument sequence-capable function.
#[inline]
pub fn seq_fn2<A0, A1, F>(f: F) -> SeqFn2<A0, A1, F> {
    SeqFn2(f, std::marker::PhantomData)
}
/// Wrap a 3-argument sequence-capable function.
#[inline]
pub fn seq_fn3<A0, A1, A2, F>(f: F) -> SeqFn3<A0, A1, A2, F> {
    SeqFn3(f, std::marker::PhantomData)
}

impl<A0, F> IntoNativeSeqHandler for SeqFn1<A0, F>
where
    A0: FromValue + 'static,
    F: Fn(A0) -> Result<NativeResult, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(1)
    }
    fn into_seq_handler(self) -> NativeSeqFn {
        Rc::new(move |args: &[Value]| {
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 0)?;
            self.0(a0)
        })
    }
}

impl<A0, A1, F> IntoNativeSeqHandler for SeqFn2<A0, A1, F>
where
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    F: Fn(A0, A1) -> Result<NativeResult, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(2)
    }
    fn into_seq_handler(self) -> NativeSeqFn {
        Rc::new(move |args: &[Value]| {
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 0)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 1)?;
            self.0(a0, a1)
        })
    }
}

impl<A0, A1, A2, F> IntoNativeSeqHandler for SeqFn3<A0, A1, A2, F>
where
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    A2: FromValue + 'static,
    F: Fn(A0, A1, A2) -> Result<NativeResult, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(3)
    }
    fn into_seq_handler(self) -> NativeSeqFn {
        Rc::new(move |args: &[Value]| {
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 0)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 1)?;
            let a2 = A2::from_value(args.get(2).ok_or("missing arg 2")?, 2)?;
            self.0(a0, a1, a2)
        })
    }
}

/// Converts a typed Rust method into a [`NativeSeqMethodFn`] that may return
/// a [`NativeResult::Sequence`] for deferred callback invocation.
pub trait IntoNativeSeqMethodHandler {
    fn arity() -> Option<u8>;
    fn into_seq_method_handler(self) -> NativeSeqMethodFn;
}

// Wrapper structs for seq methods (parallel to MFn0-MFn3).
pub struct SeqMFn0<Recv, F>(pub F, pub Ph<fn(Recv)>);
pub struct SeqMFn1<Recv, A0, F>(pub F, pub Ph<fn(Recv, A0)>);
pub struct SeqMFn2<Recv, A0, A1, F>(pub F, pub Ph<fn(Recv, A0, A1)>);
pub struct SeqMFn3<Recv, A0, A1, A2, F>(pub F, pub Ph<fn(Recv, A0, A1, A2)>);

/// Wrap a 0-arg sequence-capable method (receiver only).
#[inline]
pub fn seq_mfn0<Recv, F>(f: F) -> SeqMFn0<Recv, F> {
    SeqMFn0(f, std::marker::PhantomData)
}
/// Wrap a 1-arg sequence-capable method.
#[inline]
pub fn seq_mfn1<Recv, A0, F>(f: F) -> SeqMFn1<Recv, A0, F> {
    SeqMFn1(f, std::marker::PhantomData)
}
/// Wrap a 2-arg sequence-capable method.
#[inline]
pub fn seq_mfn2<Recv, A0, A1, F>(f: F) -> SeqMFn2<Recv, A0, A1, F> {
    SeqMFn2(f, std::marker::PhantomData)
}
/// Wrap a 3-arg sequence-capable method.
#[inline]
pub fn seq_mfn3<Recv, A0, A1, A2, F>(f: F) -> SeqMFn3<Recv, A0, A1, A2, F> {
    SeqMFn3(f, std::marker::PhantomData)
}

impl<Recv, F> IntoNativeSeqMethodHandler for SeqMFn0<Recv, F>
where
    Recv: FromValue + 'static,
    F: Fn(Recv) -> Result<NativeResult, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(0)
    }
    fn into_seq_method_handler(self) -> NativeSeqMethodFn {
        Rc::new(move |receiver: &Value, _args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            self.0(recv)
        })
    }
}

impl<Recv, A0, F> IntoNativeSeqMethodHandler for SeqMFn1<Recv, A0, F>
where
    Recv: FromValue + 'static,
    A0: FromValue + 'static,
    F: Fn(Recv, A0) -> Result<NativeResult, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(1)
    }
    fn into_seq_method_handler(self) -> NativeSeqMethodFn {
        Rc::new(move |receiver: &Value, args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 1)?;
            self.0(recv, a0)
        })
    }
}

impl<Recv, A0, A1, F> IntoNativeSeqMethodHandler for SeqMFn2<Recv, A0, A1, F>
where
    Recv: FromValue + 'static,
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    F: Fn(Recv, A0, A1) -> Result<NativeResult, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(2)
    }
    fn into_seq_method_handler(self) -> NativeSeqMethodFn {
        Rc::new(move |receiver: &Value, args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 1)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 2)?;
            self.0(recv, a0, a1)
        })
    }
}

impl<Recv, A0, A1, A2, F> IntoNativeSeqMethodHandler for SeqMFn3<Recv, A0, A1, A2, F>
where
    Recv: FromValue + 'static,
    A0: FromValue + 'static,
    A1: FromValue + 'static,
    A2: FromValue + 'static,
    F: Fn(Recv, A0, A1, A2) -> Result<NativeResult, String> + 'static,
{
    fn arity() -> Option<u8> {
        Some(3)
    }
    fn into_seq_method_handler(self) -> NativeSeqMethodFn {
        Rc::new(move |receiver: &Value, args: &[Value]| {
            let recv = Recv::from_value(receiver, 0).map_err(|e| format!("receiver: {e}"))?;
            let a0 = A0::from_value(args.first().ok_or("missing arg 0")?, 1)?;
            let a1 = A1::from_value(args.get(1).ok_or("missing arg 1")?, 2)?;
            let a2 = A2::from_value(args.get(2).ok_or("missing arg 2")?, 3)?;
            self.0(recv, a0, a1, a2)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_value_i32_direct() {
        let v = Value::I32(42);
        assert_eq!(i32::from_value(&v, 0).unwrap(), 42);
    }

    #[test]
    fn from_value_i32_widened_from_i64() {
        let v = Value::I64(100);
        assert_eq!(i32::from_value(&v, 0).unwrap(), 100_i32);
    }

    #[test]
    fn from_value_i32_overflow_err() {
        let v = Value::I64(i64::MAX);
        let err = i32::from_value(&v, 0).unwrap_err();
        assert!(err.contains("overflows i32"), "got: {err}");
    }

    #[test]
    fn from_value_i64_widens_i32() {
        let v = Value::I32(-1);
        assert_eq!(i64::from_value(&v, 0).unwrap(), -1_i64);
    }

    #[test]
    fn from_value_i64_direct() {
        let v = Value::I64(i64::MAX);
        assert_eq!(i64::from_value(&v, 0).unwrap(), i64::MAX);
    }

    #[test]
    fn from_value_f32_direct() {
        let v = Value::F32(1.5);
        assert!((f32::from_value(&v, 0).unwrap() - 1.5_f32).abs() < f32::EPSILON);
    }

    #[test]
    fn from_value_f32_narrowed_from_f64() {
        let v = Value::F64(3.125);
        let result = f32::from_value(&v, 0).unwrap();
        assert!((result - 3.125_f32).abs() < 1e-5);
    }

    #[test]
    fn from_value_f64_widens_f32() {
        let v = Value::F32(2.0);
        let result = f64::from_value(&v, 0).unwrap();
        assert!((result - 2.0_f64).abs() < f64::EPSILON);
    }

    #[test]
    fn from_value_bool_ok() {
        assert!(bool::from_value(&Value::Bool(true), 0).unwrap());
        assert!(!bool::from_value(&Value::Bool(false), 0).unwrap());
    }

    #[test]
    fn from_value_bool_wrong_type_err() {
        let err = bool::from_value(&Value::I32(1), 0).unwrap_err();
        assert!(err.contains("expected bool"), "got: {err}");
    }

    #[test]
    fn from_value_string_ok() {
        let v = Value::Str(Rc::from("hello"));
        assert_eq!(String::from_value(&v, 0).unwrap(), "hello");
    }

    #[test]
    fn from_value_string_wrong_type_err() {
        let err = String::from_value(&Value::I32(1), 0).unwrap_err();
        assert!(err.contains("expected string"), "got: {err}");
    }

    #[test]
    fn from_value_rc_str_ok() {
        let v = Value::Str(Rc::from("world"));
        let s: Rc<str> = Rc::<str>::from_value(&v, 0).unwrap();
        assert_eq!(&*s, "world");
    }

    #[test]
    fn from_value_value_passthrough() {
        let v = Value::Bool(true);
        let out = Value::from_value(&v, 0).unwrap();
        assert!(matches!(out, Value::Bool(true)));
    }

    #[test]
    fn from_value_option_value_always_some() {
        let v = Value::Null;
        let out = Option::<Value>::from_value(&v, 0).unwrap();
        assert!(out.is_some());
    }

    #[test]
    fn from_value_array_ok() {
        let arr = Rc::new(RefCell::new(vec![Value::I32(1)]));
        let v = Value::Array(Rc::clone(&arr));
        let extracted = Rc::<RefCell<Vec<Value>>>::from_value(&v, 0).unwrap();
        assert_eq!(extracted.borrow().len(), 1);
    }

    #[test]
    fn from_value_array_wrong_type_err() {
        let err = Rc::<RefCell<Vec<Value>>>::from_value(&Value::Bool(true), 0).unwrap_err();
        assert!(err.contains("expected array"), "got: {err}");
    }

    #[test]
    fn from_value_dict_ok() {
        let mut map = HashMap::new();
        map.insert("k".to_string(), Value::I32(99));
        let v = Value::Dict(Rc::new(RefCell::new(map)));
        let extracted = Rc::<RefCell<HashMap<String, Value>>>::from_value(&v, 0).unwrap();
        assert_eq!(extracted.borrow().len(), 1);
    }

    #[test]
    fn from_value_dict_wrong_type_err() {
        let err = Rc::<RefCell<HashMap<String, Value>>>::from_value(&Value::Null, 0).unwrap_err();
        assert!(err.contains("expected dictionary"), "got: {err}");
    }

    #[test]
    fn into_value_unit() {
        assert!(matches!(().into_value(), Value::Null));
    }

    #[test]
    fn into_value_bool() {
        assert!(matches!(true.into_value(), Value::Bool(true)));
    }

    #[test]
    fn into_value_i32() {
        assert!(matches!(7_i32.into_value(), Value::I32(7)));
    }

    #[test]
    fn into_value_i64() {
        assert!(matches!(i64::MAX.into_value(), Value::I64(_)));
    }

    #[test]
    fn into_value_f32() {
        assert!(matches!(1.0_f32.into_value(), Value::F32(_)));
    }

    #[test]
    fn into_value_f64() {
        assert!(matches!(1.0_f64.into_value(), Value::F64(_)));
    }

    #[test]
    fn into_value_string() {
        let v = "hi".to_string().into_value();
        assert!(matches!(v, Value::Str(_)));
    }

    #[test]
    fn into_value_option_some() {
        let v: Option<i32> = Some(5);
        assert!(matches!(v.into_value(), Value::I32(5)));
    }

    #[test]
    fn into_value_option_none() {
        let v: Option<i32> = None;
        assert!(matches!(v.into_value(), Value::Null));
    }

    #[test]
    fn fn0_handler_called() {
        let h = fn0(|| -> Result<i32, String> { Ok(42) });
        let handler = h.into_handler();
        let result = handler(&[]).unwrap();
        assert!(matches!(result, Value::I32(42)));
    }

    #[test]
    fn fn1_handler_called() {
        let h = fn1(|x: i64| -> Result<i64, String> { Ok(x * 2) });
        let handler = h.into_handler();
        let result = handler(&[Value::I32(10)]).unwrap();
        assert!(matches!(result, Value::I64(20)));
    }

    #[test]
    fn fn1_handler_type_mismatch_err() {
        let h = fn1(|_: bool| -> Result<bool, String> { Ok(true) });
        let handler = h.into_handler();
        let err = handler(&[Value::I32(1)]).unwrap_err();
        assert!(err.contains("expected bool"), "got: {err}");
    }

    #[test]
    fn fn2_handler_called() {
        let h = fn2(|a: i32, b: i32| -> Result<i32, String> { Ok(a + b) });
        let handler = h.into_handler();
        let result = handler(&[Value::I32(3), Value::I32(4)]).unwrap();
        assert!(matches!(result, Value::I32(7)));
    }

    #[test]
    fn fn3_handler_called() {
        let h = fn3(|a: i32, b: i32, c: i32| -> Result<i32, String> { Ok(a + b + c) });
        let handler = h.into_handler();
        let result = handler(&[Value::I32(1), Value::I32(2), Value::I32(3)]).unwrap();
        assert!(matches!(result, Value::I32(6)));
    }
}
