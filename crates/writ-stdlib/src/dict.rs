use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use writ_vm::{VM, Value, ValueTag};

pub fn register(vm: &mut VM) {
    vm.register_method(
        ValueTag::Dict,
        "keys",
        Some("dictionary"),
        Some(0),
        |receiver, _args| {
            let dict = extract_dict(receiver)?;
            let keys: Vec<Value> = dict
                .borrow()
                .keys()
                .map(|k| Value::Str(Rc::new(k.clone())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(keys))))
        },
    );

    vm.register_method(
        ValueTag::Dict,
        "values",
        Some("dictionary"),
        Some(0),
        |receiver, _args| {
            let dict = extract_dict(receiver)?;
            let values: Vec<Value> = dict.borrow().values().cloned().collect();
            Ok(Value::Array(Rc::new(RefCell::new(values))))
        },
    );

    vm.register_method(
        ValueTag::Dict,
        "has",
        Some("dictionary"),
        Some(1),
        |receiver, args| {
            let dict = extract_dict(receiver)?;
            let key = match &args[0] {
                Value::Str(s) => (**s).clone(),
                _ => return Err("has expects a string key".to_string()),
            };
            Ok(Value::Bool(dict.borrow().contains_key(&key)))
        },
    );

    vm.register_method(
        ValueTag::Dict,
        "remove",
        Some("dictionary"),
        Some(1),
        |receiver, args| {
            let dict = extract_dict(receiver)?;
            let key = match &args[0] {
                Value::Str(s) => (**s).clone(),
                _ => return Err("remove expects a string key".to_string()),
            };
            Ok(dict.borrow_mut().remove(&key).unwrap_or(Value::Null))
        },
    );

    vm.register_method(
        ValueTag::Dict,
        "len",
        Some("dictionary"),
        Some(0),
        |receiver, _args| {
            let dict = extract_dict(receiver)?;
            Ok(Value::I32(dict.borrow().len() as i32))
        },
    );

    vm.register_method(
        ValueTag::Dict,
        "isEmpty",
        Some("dictionary"),
        Some(0),
        |receiver, _args| {
            let dict = extract_dict(receiver)?;
            Ok(Value::Bool(dict.borrow().is_empty()))
        },
    );

    vm.register_method(
        ValueTag::Dict,
        "merge",
        Some("dictionary"),
        Some(1),
        |receiver, args| {
            let dict = extract_dict(receiver)?;
            let other = match &args[0] {
                Value::Dict(d) => d,
                _ => return Err("merge expects a dictionary argument".to_string()),
            };
            let other_entries = other.borrow();
            let mut target = dict.borrow_mut();
            for (k, v) in other_entries.iter() {
                target.insert(k.clone(), v.clone());
            }
            Ok(Value::Null)
        },
    );
}

fn extract_dict(v: &Value) -> Result<Rc<RefCell<HashMap<String, Value>>>, String> {
    match v {
        Value::Dict(d) => Ok(Rc::clone(d)),
        _ => Err(format!("expected dictionary, got {}", v.type_name())),
    }
}
