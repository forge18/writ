//! Borrow-safety utilities for the Writ VM.
//!
//! The VM uses `Rc<RefCell<T>>` for heap-allocated values (arrays, dicts,
//! objects). When the same `Rc` appears as both receiver and argument in a
//! method call, a naive `borrow()` / `borrow_mut()` will panic.
//!
//! This module provides:
//! - **`try_borrow_val` / `try_borrow_mut_val`**: safe wrappers that convert
//!   `BorrowError` into user-facing `RuntimeError` messages.
//! - **Aliasing detection**: `Rc::ptr_eq`-based checks that identify when two
//!   `Value`s share the same allocation, plus helpers to clone only the
//!   conflicting argument (zero cost in the common non-aliasing case).

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use super::value::Value;

// ---------------------------------------------------------------------------
// try_borrow helpers
// ---------------------------------------------------------------------------

/// Attempt a shared borrow, returning a user-facing error on conflict.
pub fn try_borrow_val<'a, T: ?Sized>(
    cell: &'a RefCell<T>,
    type_name: &str,
) -> Result<Ref<'a, T>, String> {
    cell.try_borrow()
        .map_err(|_| format!("cannot read {type_name} while it is being modified"))
}

/// Attempt an exclusive borrow, returning a user-facing error on conflict.
pub fn try_borrow_mut_val<'a, T: ?Sized>(
    cell: &'a RefCell<T>,
    type_name: &str,
) -> Result<RefMut<'a, T>, String> {
    cell.try_borrow_mut()
        .map_err(|_| format!("cannot modify {type_name} while it is being read"))
}

// ---------------------------------------------------------------------------
// Aliasing detection
// ---------------------------------------------------------------------------

/// Returns `true` if two `Value`s wrap the same `Rc` allocation.
///
/// Only reference-counted heap types (`Array`, `Dict`, `Object`) can alias.
/// Scalar types and value types (`Struct`) are always distinct.
pub fn values_alias(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => Rc::ptr_eq(a, b),
        (Value::Array(a), Value::Array(b)) => Rc::ptr_eq(a, b),
        (Value::Dict(a), Value::Dict(b)) => Rc::ptr_eq(a, b),
        #[cfg(feature = "mobile-aosoa")]
        (Value::AoSoA(a), Value::AoSoA(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}

/// Deep-clone a `Value`'s inner `RefCell` data into a fresh `Rc`, breaking
/// aliasing. For non-reference types this is a plain clone (no-op for scalars).
pub fn dealias_value(v: &Value) -> Value {
    match v {
        Value::Object(obj) => {
            let cloned = obj.borrow().clone_box();
            Value::Object(Rc::new(RefCell::new(cloned)))
        }
        Value::Array(arr) => Value::Array(Rc::new(RefCell::new(arr.borrow().clone()))),
        Value::Dict(dict) => Value::Dict(Rc::new(RefCell::new(dict.borrow().clone()))),
        #[cfg(feature = "mobile-aosoa")]
        Value::AoSoA(c) => Value::AoSoA(Rc::new(RefCell::new(c.borrow().clone()))),
        other => other.clone(),
    }
}

// ---------------------------------------------------------------------------
// Method dispatch helpers (receiver + args)
// ---------------------------------------------------------------------------

/// Returns `true` if **any** argument aliases the receiver.
#[inline]
pub fn has_receiver_aliasing(receiver: &Value, args: &[Value]) -> bool {
    args.iter().any(|arg| values_alias(arg, receiver))
}

/// Clone args, dealiasing any that point to the same `Rc` as the receiver.
/// Only call this when [`has_receiver_aliasing`] returned `true`.
pub fn dealias_args(receiver: &Value, args: &[Value]) -> Vec<Value> {
    args.iter()
        .map(|arg| {
            if values_alias(arg, receiver) {
                dealias_value(arg)
            } else {
                arg.clone()
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Function dispatch helpers (pairwise args, no receiver)
// ---------------------------------------------------------------------------

/// Check all argument pairs for aliasing. Returns `None` if clean,
/// `Some(dealiased)` if any pair shares an `Rc`.
///
/// With max 3 args (`fn3`) this is at most 3 pointer comparisons.
pub fn dealias_fn_args(args: &[Value]) -> Option<Vec<Value>> {
    for i in 0..args.len() {
        for j in (i + 1)..args.len() {
            if values_alias(&args[i], &args[j]) {
                // Clone the whole slice and dealias every later duplicate.
                let mut safe = args.to_vec();
                for k in (i + 1)..safe.len() {
                    if values_alias(&args[i], &safe[k]) {
                        safe[k] = dealias_value(&args[k]);
                    }
                }
                return Some(safe);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn scalars_never_alias() {
        assert!(!values_alias(&Value::I32(1), &Value::I32(1)));
        assert!(!values_alias(&Value::Null, &Value::Null));
    }

    #[test]
    fn same_array_aliases() {
        let arr = Value::Array(Rc::new(RefCell::new(vec![Value::I32(1)])));
        assert!(values_alias(&arr, &arr));
    }

    #[test]
    fn different_arrays_do_not_alias() {
        let a = Value::Array(Rc::new(RefCell::new(vec![Value::I32(1)])));
        let b = Value::Array(Rc::new(RefCell::new(vec![Value::I32(1)])));
        assert!(!values_alias(&a, &b));
    }

    #[test]
    fn same_dict_aliases() {
        let dict = Value::Dict(Rc::new(RefCell::new(HashMap::new())));
        assert!(values_alias(&dict, &dict));
    }

    #[test]
    fn dealias_breaks_aliasing() {
        let arr = Value::Array(Rc::new(RefCell::new(vec![Value::I32(42)])));
        let dealiased = dealias_value(&arr);
        assert!(!values_alias(&arr, &dealiased));
        // Content should be identical
        if let (Value::Array(a), Value::Array(b)) = (&arr, &dealiased) {
            assert_eq!(*a.borrow(), *b.borrow());
        }
    }

    #[test]
    fn dealias_args_only_clones_aliased() {
        let arr = Value::Array(Rc::new(RefCell::new(vec![Value::I32(1)])));
        let other = Value::I32(99);
        let result = dealias_args(&arr, &[arr.clone(), other.clone()]);
        // First arg was aliased — should be dealiased
        assert!(!values_alias(&arr, &result[0]));
        // Second arg was not aliased — should be unchanged
        assert_eq!(result[1], Value::I32(99));
    }

    #[test]
    fn dealias_fn_args_detects_pairwise() {
        let arr = Value::Array(Rc::new(RefCell::new(vec![Value::I32(1)])));
        let result = dealias_fn_args(&[arr.clone(), arr.clone()]);
        assert!(result.is_some());
        let safe = result.unwrap();
        assert!(!values_alias(&safe[0], &safe[1]));
    }

    #[test]
    fn dealias_fn_args_clean_returns_none() {
        let a = Value::Array(Rc::new(RefCell::new(vec![Value::I32(1)])));
        let b = Value::Array(Rc::new(RefCell::new(vec![Value::I32(2)])));
        assert!(dealias_fn_args(&[a, b]).is_none());
    }
}
