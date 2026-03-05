use std::cell::RefCell;
use std::rc::Rc;

use writ_vm::{VM, Value, ValueTag};

pub fn register(vm: &mut VM) {
    vm.register_method(
        ValueTag::Array,
        "push",
        Some("array"),
        Some(1),
        |receiver, args| {
            let arr = extract_array(receiver)?;
            arr.borrow_mut().push(args[0].clone());
            Ok(Value::Null)
        },
    );

    vm.register_method(
        ValueTag::Array,
        "pop",
        Some("array"),
        Some(0),
        |receiver, _args| {
            let arr = extract_array(receiver)?;
            Ok(arr.borrow_mut().pop().unwrap_or(Value::Null))
        },
    );

    vm.register_method(
        ValueTag::Array,
        "insert",
        Some("array"),
        Some(2),
        |receiver, args| {
            let arr = extract_array(receiver)?;
            let idx = extract_index(&args[0], "insert")?;
            let mut items = arr.borrow_mut();
            if idx > items.len() {
                return Err(format!(
                    "insert index {} out of bounds (len {})",
                    idx,
                    items.len()
                ));
            }
            items.insert(idx, args[1].clone());
            Ok(Value::Null)
        },
    );

    vm.register_method(
        ValueTag::Array,
        "remove",
        Some("array"),
        Some(1),
        |receiver, args| {
            let arr = extract_array(receiver)?;
            let idx = extract_index(&args[0], "remove")?;
            let mut items = arr.borrow_mut();
            if idx >= items.len() {
                return Err(format!(
                    "remove index {} out of bounds (len {})",
                    idx,
                    items.len()
                ));
            }
            Ok(items.remove(idx))
        },
    );

    vm.register_method(
        ValueTag::Array,
        "len",
        Some("array"),
        Some(0),
        |receiver, _args| {
            let arr = extract_array(receiver)?;
            Ok(Value::I32(arr.borrow().len() as i32))
        },
    );

    vm.register_method(
        ValueTag::Array,
        "isEmpty",
        Some("array"),
        Some(0),
        |receiver, _args| {
            let arr = extract_array(receiver)?;
            Ok(Value::Bool(arr.borrow().is_empty()))
        },
    );

    vm.register_method(
        ValueTag::Array,
        "contains",
        Some("array"),
        Some(1),
        |receiver, args| {
            let arr = extract_array(receiver)?;
            let items = arr.borrow();
            Ok(Value::Bool(items.iter().any(|v| v == &args[0])))
        },
    );

    vm.register_method(
        ValueTag::Array,
        "indexOf",
        Some("array"),
        Some(1),
        |receiver, args| {
            let arr = extract_array(receiver)?;
            let items = arr.borrow();
            match items.iter().position(|v| v == &args[0]) {
                Some(pos) => Ok(Value::I32(pos as i32)),
                None => Ok(Value::I32(-1)),
            }
        },
    );

    vm.register_method(
        ValueTag::Array,
        "reverse",
        Some("array"),
        Some(0),
        |receiver, _args| {
            let arr = extract_array(receiver)?;
            arr.borrow_mut().reverse();
            Ok(Value::Null)
        },
    );

    vm.register_method(
        ValueTag::Array,
        "sort",
        Some("array"),
        Some(0),
        |receiver, _args| {
            let arr = extract_array(receiver)?;
            let mut items = arr.borrow_mut();
            items.sort_by(compare_values);
            Ok(Value::Null)
        },
    );

    vm.register_method(
        ValueTag::Array,
        "first",
        Some("array"),
        Some(0),
        |receiver, _args| {
            let arr = extract_array(receiver)?;
            let items = arr.borrow();
            Ok(items.first().cloned().unwrap_or(Value::Null))
        },
    );

    vm.register_method(
        ValueTag::Array,
        "last",
        Some("array"),
        Some(0),
        |receiver, _args| {
            let arr = extract_array(receiver)?;
            let items = arr.borrow();
            Ok(items.last().cloned().unwrap_or(Value::Null))
        },
    );

    vm.register_method(
        ValueTag::Array,
        "slice",
        Some("array"),
        Some(2),
        |receiver, args| {
            let arr = extract_array(receiver)?;
            let start = extract_index(&args[0], "slice")?;
            let end = extract_index(&args[1], "slice")?;
            let items = arr.borrow();
            let end = end.min(items.len());
            let start = start.min(end);
            let sliced: Vec<Value> = items[start..end].to_vec();
            Ok(Value::Array(Rc::new(RefCell::new(sliced))))
        },
    );

    // NOTE: map, filter, reduce are handled as built-in callback methods
    // in the VM's exec_call_method since they need &mut VM access.

    vm.register_method(
        ValueTag::Array,
        "join",
        Some("array"),
        Some(1),
        |receiver, args| {
            let arr = extract_array(receiver)?;
            let separator = match &args[0] {
                Value::Str(s) => (**s).clone(),
                _ => return Err("join expects a string separator".to_string()),
            };
            let items = arr.borrow();
            let parts: Vec<String> = items
                .iter()
                .map(|v| match v {
                    Value::Str(s) => (**s).clone(),
                    other => other.to_string(),
                })
                .collect();
            Ok(Value::Str(Rc::new(parts.join(&separator))))
        },
    );
}

fn extract_array(v: &Value) -> Result<Rc<RefCell<Vec<Value>>>, String> {
    match v {
        Value::Array(a) => Ok(Rc::clone(a)),
        _ => Err(format!("expected array, got {}", v.type_name())),
    }
}

fn extract_index(v: &Value, method: &str) -> Result<usize, String> {
    match v {
        v @ (Value::I32(_) | Value::I64(_)) => {
            let i = v.as_i64();
            if i < 0 {
                Err(format!("{method} expects a non-negative integer index"))
            } else {
                Ok(i as usize)
            }
        }
        _ => Err(format!("{method} expects an integer index")),
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
