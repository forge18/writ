use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::binding::{mfn0, mfn1, mfn2};
use crate::vm::{VM, Value, ValueTag};

type Arr = Rc<RefCell<Vec<Value>>>;

pub fn register(vm: &mut VM) {
    vm.register_method(
        ValueTag::Array,
        "push",
        Some("array"),
        mfn1(|arr: Arr, item: Value| -> Result<(), String> {
            arr.borrow_mut().push(item);
            Ok(())
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "pop",
        Some("array"),
        mfn0(|arr: Arr| -> Result<Value, String> {
            Ok(arr.borrow_mut().pop().unwrap_or(Value::Null))
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "insert",
        Some("array"),
        mfn2(|arr: Arr, idx: i64, item: Value| -> Result<(), String> {
            let idx = to_index(idx, "insert")?;
            let mut items = arr.borrow_mut();
            if idx > items.len() {
                return Err(format!(
                    "insert index {} out of bounds (len {})",
                    idx,
                    items.len()
                ));
            }
            items.insert(idx, item);
            Ok(())
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "remove",
        Some("array"),
        mfn1(|arr: Arr, idx: i64| -> Result<Value, String> {
            let idx = to_index(idx, "remove")?;
            let mut items = arr.borrow_mut();
            if idx >= items.len() {
                return Err(format!(
                    "remove index {} out of bounds (len {})",
                    idx,
                    items.len()
                ));
            }
            Ok(items.remove(idx))
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "len",
        Some("array"),
        mfn0(|arr: Arr| -> Result<i32, String> { Ok(arr.borrow().len() as i32) }),
    );

    vm.register_method(
        ValueTag::Array,
        "isEmpty",
        Some("array"),
        mfn0(|arr: Arr| -> Result<bool, String> { Ok(arr.borrow().is_empty()) }),
    );

    vm.register_method(
        ValueTag::Array,
        "contains",
        Some("array"),
        mfn1(|arr: Arr, item: Value| -> Result<bool, String> {
            Ok(arr.borrow().iter().any(|v| v == &item))
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "indexOf",
        Some("array"),
        mfn1(|arr: Arr, item: Value| -> Result<i32, String> {
            Ok(match arr.borrow().iter().position(|v| v == &item) {
                Some(pos) => pos as i32,
                None => -1,
            })
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "reverse",
        Some("array"),
        mfn0(|arr: Arr| -> Result<(), String> {
            arr.borrow_mut().reverse();
            Ok(())
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "sort",
        Some("array"),
        mfn0(|arr: Arr| -> Result<(), String> {
            arr.borrow_mut().sort_by(compare_values);
            Ok(())
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "first",
        Some("array"),
        mfn0(|arr: Arr| -> Result<Value, String> {
            Ok(arr.borrow().first().cloned().unwrap_or(Value::Null))
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "last",
        Some("array"),
        mfn0(|arr: Arr| -> Result<Value, String> {
            Ok(arr.borrow().last().cloned().unwrap_or(Value::Null))
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "slice",
        Some("array"),
        mfn2(|arr: Arr, start: i64, end: i64| -> Result<Value, String> {
            let start = to_index(start, "slice")?;
            let end = to_index(end, "slice")?;
            let items = arr.borrow();
            let end = end.min(items.len());
            let start = start.min(end);
            let sliced: Vec<Value> = items[start..end].to_vec();
            Ok(Value::Array(Rc::new(RefCell::new(sliced))))
        }),
    );

    vm.register_method(
        ValueTag::Array,
        "join",
        Some("array"),
        mfn1(
            |arr: Arr, separator: Rc<str>| -> Result<String, String> {
                let items = arr.borrow();
                let parts: Vec<String> = items
                    .iter()
                    .map(|v| match v {
                        Value::Str(s) => s.to_string(),
                        other => other.to_string(),
                    })
                    .collect();
                Ok(parts.join(&*separator))
            },
        ),
    );
}

fn to_index(i: i64, method: &str) -> Result<usize, String> {
    if i < 0 {
        Err(format!("{method} expects a non-negative integer index"))
    } else {
        Ok(i as usize)
    }
}

fn compare_values(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
            a.as_i64().cmp(&b.as_i64())
        }
        (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::F32(_) | Value::F64(_))) => a
            .as_f64()
            .partial_cmp(&b.as_f64())
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::Str(a), Value::Str(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        _ => std::cmp::Ordering::Equal,
    }
}
