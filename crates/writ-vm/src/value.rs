use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

#[cfg(feature = "mobile-aosoa")]
use crate::aosoa::AoSoAContainer;
use crate::object::WritObject;
use crate::writ_struct::WritStruct;

/// Heap-allocated closure data, boxed to keep the `Value` enum small.
#[derive(Debug, Clone)]
pub struct ClosureData {
    /// Index into the VM's function table.
    pub func_idx: usize,
    /// Captured upvalue cells shared with enclosing scopes.
    pub upvalues: Vec<Rc<RefCell<Value>>>,
}

/// A runtime value in the Writ VM.
///
/// Integer and float widths are flattened directly into the enum to
/// eliminate nested discriminant checks on the arithmetic hot path.
/// Width promotion (i32→i64, f32→f64) still happens transparently.
#[derive(Debug, Clone)]
pub enum Value {
    /// 32-bit signed integer — the default narrow representation.
    I32(i32),
    /// 64-bit signed integer — promoted to when i32 overflows.
    I64(i64),
    /// 32-bit float — the default narrow representation.
    F32(f32),
    /// 64-bit float — promoted to when f32 range/precision is exceeded.
    F64(f64),
    /// Boolean.
    Bool(bool),
    /// Reference-counted string.
    Str(Rc<String>),
    /// Null value.
    Null,
    /// Reference-counted mutable array.
    Array(Rc<RefCell<Vec<Value>>>),
    /// Reference-counted mutable dictionary with string keys.
    Dict(Rc<RefCell<HashMap<String, Value>>>),
    /// Host-owned object implementing the [`WritObject`] trait.
    Object(Rc<RefCell<dyn WritObject>>),
    /// Writ struct instance — true value type, copied on assignment.
    Struct(Box<WritStruct>),
    /// A closure: a function bundled with its captured upvalues (boxed to
    /// keep Value small — closures are rare on the hot path).
    Closure(Box<ClosureData>),
    /// Handle to a running coroutine, used for yield-coroutine chains.
    CoroutineHandle(u64),
    /// AoSoA container: cache-friendly columnar storage for homogeneous struct arrays.
    /// Only available when the `mobile-aosoa` feature is enabled.
    #[cfg(feature = "mobile-aosoa")]
    AoSoA(Rc<RefCell<AoSoAContainer>>),
}

/// Lightweight type tag for method dispatch keying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueTag {
    Int,
    Float,
    Bool,
    Str,
    Array,
    Dict,
    Object,
    Struct,
}

impl Value {
    /// Returns the type tag for method dispatch.
    pub fn tag(&self) -> Option<ValueTag> {
        match self {
            Value::I32(_) | Value::I64(_) => Some(ValueTag::Int),
            Value::F32(_) | Value::F64(_) => Some(ValueTag::Float),
            Value::Bool(_) => Some(ValueTag::Bool),
            Value::Str(_) => Some(ValueTag::Str),
            Value::Array(_) => Some(ValueTag::Array),
            Value::Dict(_) => Some(ValueTag::Dict),
            Value::Object(_) => Some(ValueTag::Object),
            Value::Struct(_) => Some(ValueTag::Struct),
            #[cfg(feature = "mobile-aosoa")]
            Value::AoSoA(_) => Some(ValueTag::Array),
            Value::Null | Value::CoroutineHandle(_) | Value::Closure(_) => None,
        }
    }

    // ── Integer helpers ──────────────────────────────────────────

    /// Returns `true` if this value is an integer (either width).
    #[inline(always)]
    pub fn is_int(&self) -> bool {
        matches!(self, Value::I32(_) | Value::I64(_))
    }

    /// Returns `true` if this value is a float (either width).
    #[inline(always)]
    pub fn is_float(&self) -> bool {
        matches!(self, Value::F32(_) | Value::F64(_))
    }

    /// Returns `true` if this value is numeric (int or float).
    #[inline(always)]
    pub fn is_numeric(&self) -> bool {
        self.is_int() || self.is_float()
    }

    /// Returns the integer value as i64, widening if necessary.
    /// Panics if not an integer.
    #[inline(always)]
    pub fn as_i64(&self) -> i64 {
        match self {
            Value::I32(v) => *v as i64,
            Value::I64(v) => *v,
            _ => unreachable!(),
        }
    }

    /// Returns the float value as f64, widening if necessary.
    /// Panics if not a float.
    #[inline(always)]
    pub fn as_f64(&self) -> f64 {
        match self {
            Value::F32(v) => *v as f64,
            Value::F64(v) => *v,
            _ => unreachable!(),
        }
    }

    /// Applies an operation to two float values, promoting to the wider width.
    /// If both are F32, stays F32. If either is F64, promotes to F64.
    #[inline(always)]
    pub fn promote_float_pair_op(a: &Value, b: &Value, op: fn(f64, f64) -> f64) -> Value {
        match (a, b) {
            (Value::F32(a), Value::F32(b)) => {
                let result = op(*a as f64, *b as f64);
                // If result fits f32 without overflow, keep narrow
                let narrow = result as f32;
                if narrow.is_finite() || !result.is_finite() {
                    Value::F32(narrow)
                } else {
                    // Result overflowed f32 — promote
                    Value::F64(result)
                }
            }
            _ => Value::F64(op(a.as_f64(), b.as_f64())),
        }
    }

    /// Cheaply duplicates the value. For scalar types (I32, I64, F32, F64,
    /// Bool, Null) this is a bitwise copy with no Rc refcount overhead.
    /// For heap types (Str, Array, Dict, etc.) this falls back to Clone (Rc bump).
    #[inline(always)]
    pub fn cheap_clone(&self) -> Value {
        match self {
            Value::I32(v) => Value::I32(*v),
            Value::I64(v) => Value::I64(*v),
            Value::F32(v) => Value::F32(*v),
            Value::F64(v) => Value::F64(*v),
            Value::Bool(b) => Value::Bool(*b),
            Value::Null => Value::Null,
            other => other.clone(),
        }
    }

    /// Returns `true` if the value is falsy (null or false).
    #[inline(always)]
    pub fn is_falsy(&self) -> bool {
        matches!(self, Value::Null | Value::Bool(false))
    }

    /// Returns `true` if the value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Returns an owned type name — handles `Object` by borrowing the Rc.
    pub fn type_name_owned(&self) -> String {
        match self {
            Value::Object(obj) => obj.borrow().type_name().to_string(),
            other => other.type_name().to_string(),
        }
    }

    /// Returns a human-readable type name for error messages.
    ///
    /// Script authors see only `"int"` and `"float"` regardless of the
    /// internal width — the promotion is an implementation detail.
    pub fn type_name(&self) -> &str {
        match self {
            Value::I32(_) | Value::I64(_) => "int",
            Value::F32(_) | Value::F64(_) => "float",
            Value::Bool(_) => "bool",
            Value::Str(_) => "string",
            Value::Null => "null",
            Value::Array(_) => "array",
            Value::Dict(_) => "dict",
            Value::Object(_) => "object",
            Value::Struct(s) => &s.layout.type_name,
            Value::Closure(_) => "function",
            #[cfg(feature = "mobile-aosoa")]
            Value::AoSoA(_) => "array",
            Value::CoroutineHandle(_) => "coroutine",
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I32(a), Value::I32(b)) => a == b,
            (Value::I32(a), Value::I64(b)) => *a as i64 == *b,
            (Value::I64(a), Value::I32(b)) => *a == *b as i64,
            (Value::I64(a), Value::I64(b)) => a == b,
            (Value::F32(a), Value::F32(b)) => a.to_bits() == b.to_bits(),
            (Value::F32(a), Value::F64(b)) => (*a as f64).to_bits() == b.to_bits(),
            (Value::F64(a), Value::F32(b)) => a.to_bits() == (*b as f64).to_bits(),
            (Value::F64(a), Value::F64(b)) => a.to_bits() == b.to_bits(),
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
            (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
            (Value::Struct(a), Value::Struct(b)) => a == b,
            (Value::Closure(a), Value::Closure(b)) => {
                a.func_idx == b.func_idx
                    && a.upvalues.len() == b.upvalues.len()
                    && a.upvalues
                        .iter()
                        .zip(b.upvalues.iter())
                        .all(|(a, b)| Rc::ptr_eq(a, b))
            }
            (Value::CoroutineHandle(a), Value::CoroutineHandle(b)) => a == b,
            #[cfg(feature = "mobile-aosoa")]
            (Value::AoSoA(a), Value::AoSoA(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Eq for Value {}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Use canonical type tags so cross-width values hash identically
        // (I32(42) == I64(42) must imply same hash).
        match self {
            Value::I32(v) => {
                0u8.hash(state);
                (*v as i64).hash(state);
            }
            Value::I64(v) => {
                0u8.hash(state);
                v.hash(state);
            }
            Value::F32(v) => {
                1u8.hash(state);
                (*v as f64).to_bits().hash(state);
            }
            Value::F64(v) => {
                1u8.hash(state);
                v.to_bits().hash(state);
            }
            Value::Bool(v) => {
                2u8.hash(state);
                v.hash(state);
            }
            Value::Str(v) => {
                3u8.hash(state);
                v.hash(state);
            }
            Value::Null => 4u8.hash(state),
            Value::Array(v) => {
                5u8.hash(state);
                Rc::as_ptr(v).hash(state);
            }
            Value::Dict(v) => {
                6u8.hash(state);
                Rc::as_ptr(v).hash(state);
            }
            Value::Object(v) => {
                7u8.hash(state);
                (Rc::as_ptr(v) as *const ()).hash(state);
            }
            Value::Struct(s) => {
                8u8.hash(state);
                s.layout.type_name.hash(state);
                for (i, name) in s.layout.field_names.iter().enumerate() {
                    name.hash(state);
                    s.fields[i].hash(state);
                }
            }
            Value::Closure(data) => {
                9u8.hash(state);
                data.func_idx.hash(state);
            }
            Value::CoroutineHandle(v) => {
                10u8.hash(state);
                v.hash(state);
            }
            #[cfg(feature = "mobile-aosoa")]
            Value::AoSoA(v) => {
                11u8.hash(state);
                Rc::as_ptr(v).hash(state);
            }
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I32(v) => write!(f, "{v}"),
            Value::I64(v) => write!(f, "{v}"),
            Value::F32(v) => write!(f, "{v}"),
            Value::F64(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Str(v) => write!(f, "{v}"),
            Value::Null => write!(f, "null"),
            Value::Array(v) => {
                let items = v.borrow();
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Dict(v) => {
                let entries = v.borrow();
                write!(f, "{{")?;
                for (i, (k, val)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {val}")?;
                }
                write!(f, "}}")
            }
            Value::Object(obj) => {
                let obj = obj.borrow();
                write!(f, "<{}>", obj.type_name())
            }
            Value::Struct(s) => {
                write!(f, "{}(", s.layout.type_name)?;
                for (i, name) in s.layout.field_names.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {}", s.fields[i])?;
                }
                write!(f, ")")
            }
            Value::Closure(data) => write!(f, "<closure:{}>", data.func_idx),
            Value::CoroutineHandle(id) => write!(f, "<coroutine:{id}>"),
            #[cfg(feature = "mobile-aosoa")]
            Value::AoSoA(container) => write!(f, "{}", container.borrow()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_int() {
        assert_eq!(Value::I32(42).to_string(), "42");
        assert_eq!(Value::I64(42).to_string(), "42");
    }

    #[test]
    fn test_display_float() {
        assert_eq!(Value::F32(3.14).to_string(), "3.14");
        assert_eq!(Value::F64(3.14).to_string(), "3.14");
    }

    #[test]
    fn test_display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn test_display_string() {
        let v = Value::Str(Rc::new("hello".to_string()));
        assert_eq!(v.to_string(), "hello");
    }

    #[test]
    fn test_display_null() {
        assert_eq!(Value::Null.to_string(), "null");
    }

    #[test]
    fn test_display_array() {
        let arr = Value::Array(Rc::new(RefCell::new(vec![
            Value::I32(1),
            Value::I32(2),
            Value::I32(3),
        ])));
        assert_eq!(arr.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn test_is_falsy() {
        assert!(Value::Null.is_falsy());
        assert!(Value::Bool(false).is_falsy());
        assert!(!Value::Bool(true).is_falsy());
        assert!(!Value::I32(0).is_falsy());
        assert!(!Value::Str(Rc::new(String::new())).is_falsy());
    }

    #[test]
    fn test_equality() {
        assert_eq!(Value::I32(42), Value::I32(42));
        assert_ne!(Value::I32(1), Value::I32(2));
        // Cross-width equality: I32(42) == I64(42)
        assert_eq!(Value::I32(42), Value::I64(42));
        assert_ne!(Value::I32(1), Value::F32(1.0));
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(
            Value::Str(Rc::new("hello".to_string())),
            Value::Str(Rc::new("hello".to_string()))
        );
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;
        fn hash(v: &Value) -> u64 {
            let mut hasher = DefaultHasher::new();
            v.hash(&mut hasher);
            hasher.finish()
        }
        let a = Value::I32(42);
        let b = Value::I32(42);
        assert_eq!(hash(&a), hash(&b));
        // Cross-width hash consistency
        let c = Value::I64(42);
        assert_eq!(hash(&a), hash(&c));
    }

    #[test]
    fn test_type_name() {
        assert_eq!(Value::I32(0).type_name(), "int");
        assert_eq!(Value::I64(0).type_name(), "int");
        assert_eq!(Value::F32(0.0).type_name(), "float");
        assert_eq!(Value::F64(0.0).type_name(), "float");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Str(Rc::new(String::new())).type_name(), "string");
        assert_eq!(Value::Null.type_name(), "null");
    }

    #[test]
    fn test_int_helpers() {
        assert_eq!(Value::I32(42).as_i64(), 42);
        assert_eq!(Value::I64(42).as_i64(), 42);
        assert!(Value::I32(42).is_int());
        assert!(Value::I64(42).is_int());
        assert!(!Value::F32(1.0).is_int());
    }

    #[test]
    fn test_float_helpers() {
        assert!((Value::F32(3.14).as_f64() - 3.14f32 as f64).abs() < f64::EPSILON);
        assert_eq!(Value::F64(3.14).as_f64(), 3.14);
        assert!(Value::F32(1.0).is_float());
        assert!(Value::F64(1.0).is_float());
        assert!(!Value::I32(1).is_float());
    }

    #[test]
    fn test_cross_width_int_equality() {
        assert_eq!(Value::I32(100), Value::I64(100));
        assert_ne!(Value::I32(100), Value::I64(200));
    }

    #[test]
    fn test_cross_width_float_equality() {
        assert_eq!(Value::F32(1.0), Value::F64(1.0));
    }

    #[test]
    fn test_value_size() {
        let size = std::mem::size_of::<Value>();
        eprintln!("sizeof(Value) = {size}");
        // Value should be at most 24 bytes (fat pointer variant Object).
        // If Object is boxed, it should be 16 bytes.
        assert!(size <= 24, "Value is {size} bytes, expected <= 24");
    }
}
