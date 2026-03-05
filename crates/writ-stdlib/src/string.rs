use std::cell::RefCell;
use std::rc::Rc;

use writ_vm::{VM, Value, ValueTag};

pub fn register(vm: &mut VM) {
    vm.register_method(
        ValueTag::Str,
        "len",
        Some("string"),
        Some(0),
        |receiver, _args| {
            let s = extract_str(receiver)?;
            Ok(Value::I32(s.len() as i32))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "trim",
        Some("string"),
        Some(0),
        |receiver, _args| {
            let s = extract_str(receiver)?;
            Ok(Value::Str(Rc::new(s.trim().to_string())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "trimStart",
        Some("string"),
        Some(0),
        |receiver, _args| {
            let s = extract_str(receiver)?;
            Ok(Value::Str(Rc::new(s.trim_start().to_string())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "trimEnd",
        Some("string"),
        Some(0),
        |receiver, _args| {
            let s = extract_str(receiver)?;
            Ok(Value::Str(Rc::new(s.trim_end().to_string())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "toUpper",
        Some("string"),
        Some(0),
        |receiver, _args| {
            let s = extract_str(receiver)?;
            Ok(Value::Str(Rc::new(s.to_uppercase())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "toLower",
        Some("string"),
        Some(0),
        |receiver, _args| {
            let s = extract_str(receiver)?;
            Ok(Value::Str(Rc::new(s.to_lowercase())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "contains",
        Some("string"),
        Some(1),
        |receiver, args| {
            let s = extract_str(receiver)?;
            let needle = extract_str(&args[0])?;
            Ok(Value::Bool(s.contains(needle.as_str())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "startsWith",
        Some("string"),
        Some(1),
        |receiver, args| {
            let s = extract_str(receiver)?;
            let prefix = extract_str(&args[0])?;
            Ok(Value::Bool(s.starts_with(prefix.as_str())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "endsWith",
        Some("string"),
        Some(1),
        |receiver, args| {
            let s = extract_str(receiver)?;
            let suffix = extract_str(&args[0])?;
            Ok(Value::Bool(s.ends_with(suffix.as_str())))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "replace",
        Some("string"),
        Some(2),
        |receiver, args| {
            let s = extract_str(receiver)?;
            let old = extract_str(&args[0])?;
            let new = extract_str(&args[1])?;
            Ok(Value::Str(Rc::new(s.replace(old.as_str(), new.as_str()))))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "split",
        Some("string"),
        Some(1),
        |receiver, args| {
            let s = extract_str(receiver)?;
            let separator = extract_str(&args[0])?;
            let parts: Vec<Value> = s
                .split(separator.as_str())
                .map(|part| Value::Str(Rc::new(part.to_string())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(parts))))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "join",
        Some("string"),
        Some(1),
        |receiver, args| {
            let separator = extract_str(receiver)?;
            let arr = match &args[0] {
                Value::Array(a) => a.borrow(),
                _ => return Err("join expects an array argument".to_string()),
            };
            let parts: Vec<String> = arr
                .iter()
                .map(|v| match v {
                    Value::Str(s) => (**s).clone(),
                    other => other.to_string(),
                })
                .collect();
            Ok(Value::Str(Rc::new(parts.join(separator.as_str()))))
        },
    );

    vm.register_method(
        ValueTag::Str,
        "charAt",
        Some("string"),
        Some(1),
        |receiver, args| {
            let s = extract_str(receiver)?;
            let idx = match &args[0] {
                v @ (Value::I32(_) | Value::I64(_)) => v.as_i64() as usize,
                _ => return Err("charAt expects an integer index".to_string()),
            };
            match s.chars().nth(idx) {
                Some(c) => Ok(Value::Str(Rc::new(c.to_string()))),
                None => Ok(Value::Null),
            }
        },
    );

    vm.register_method(
        ValueTag::Str,
        "indexOf",
        Some("string"),
        Some(1),
        |receiver, args| {
            let s = extract_str(receiver)?;
            let needle = extract_str(&args[0])?;
            match s.find(needle.as_str()) {
                Some(pos) => Ok(Value::I32(pos as i32)),
                None => Ok(Value::I32(-1)),
            }
        },
    );

    vm.register_method(
        ValueTag::Str,
        "parse",
        Some("string"),
        Some(0),
        |receiver, _args| {
            let s = extract_str(receiver)?;
            let trimmed = s.trim();
            if let Ok(i) = trimmed.parse::<i64>() {
                if let Ok(v) = i32::try_from(i) {
                    return Ok(Value::I32(v));
                }
                return Ok(Value::I64(i));
            }
            if let Ok(f) = trimmed.parse::<f64>() {
                return Ok(Value::F64(f));
            }
            Err(format!("cannot parse '{}' as a number", s))
        },
    );
}

fn extract_str(v: &Value) -> Result<Rc<String>, String> {
    match v {
        Value::Str(s) => Ok(Rc::clone(s)),
        _ => Err(format!("expected string, got {}", v.type_name())),
    }
}
