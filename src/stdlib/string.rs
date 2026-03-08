use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::binding::{mfn0, mfn1, mfn2};
use crate::vm::{VM, Value, ValueTag};

type RcStr = Rc<str>;
type Arr = Rc<RefCell<Vec<Value>>>;

pub fn register(vm: &mut VM) {
    vm.register_method(
        ValueTag::Str,
        "len",
        Some("string"),
        mfn0(|s: RcStr| -> Result<i32, String> { Ok(s.len() as i32) }),
    );

    vm.register_method(
        ValueTag::Str,
        "trim",
        Some("string"),
        mfn0(|s: RcStr| -> Result<String, String> { Ok(s.trim().to_string()) }),
    );

    vm.register_method(
        ValueTag::Str,
        "trimStart",
        Some("string"),
        mfn0(|s: RcStr| -> Result<String, String> { Ok(s.trim_start().to_string()) }),
    );

    vm.register_method(
        ValueTag::Str,
        "trimEnd",
        Some("string"),
        mfn0(|s: RcStr| -> Result<String, String> { Ok(s.trim_end().to_string()) }),
    );

    vm.register_method(
        ValueTag::Str,
        "toUpper",
        Some("string"),
        mfn0(|s: RcStr| -> Result<String, String> { Ok(s.to_uppercase()) }),
    );

    vm.register_method(
        ValueTag::Str,
        "toLower",
        Some("string"),
        mfn0(|s: RcStr| -> Result<String, String> { Ok(s.to_lowercase()) }),
    );

    vm.register_method(
        ValueTag::Str,
        "contains",
        Some("string"),
        mfn1(|s: RcStr, needle: RcStr| -> Result<bool, String> { Ok(s.contains(&*needle)) }),
    );

    vm.register_method(
        ValueTag::Str,
        "startsWith",
        Some("string"),
        mfn1(|s: RcStr, prefix: RcStr| -> Result<bool, String> { Ok(s.starts_with(&*prefix)) }),
    );

    vm.register_method(
        ValueTag::Str,
        "endsWith",
        Some("string"),
        mfn1(|s: RcStr, suffix: RcStr| -> Result<bool, String> { Ok(s.ends_with(&*suffix)) }),
    );

    vm.register_method(
        ValueTag::Str,
        "replace",
        Some("string"),
        mfn2(
            |s: RcStr, old: RcStr, new: RcStr| -> Result<String, String> {
                Ok(s.replace(&*old, &new))
            },
        ),
    );

    vm.register_method(
        ValueTag::Str,
        "split",
        Some("string"),
        mfn1(|s: RcStr, separator: RcStr| -> Result<Value, String> {
            let parts: Vec<Value> = s
                .split(&*separator)
                .map(|part| Value::Str(Rc::from(part)))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(parts))))
        }),
    );

    vm.register_method(
        ValueTag::Str,
        "join",
        Some("string"),
        mfn1(|separator: RcStr, arr: Arr| -> Result<String, String> {
            let items = arr.borrow();
            let parts: Vec<String> = items
                .iter()
                .map(|v| match v {
                    Value::Str(s) => s.to_string(),
                    other => other.to_string(),
                })
                .collect();
            Ok(parts.join(&*separator))
        }),
    );

    vm.register_method(
        ValueTag::Str,
        "charAt",
        Some("string"),
        mfn1(|s: RcStr, idx: i64| -> Result<Value, String> {
            match s.chars().nth(idx as usize) {
                Some(c) => Ok(Value::Str(Rc::from(c.to_string().as_str()))),
                None => Ok(Value::Null),
            }
        }),
    );

    vm.register_method(
        ValueTag::Str,
        "indexOf",
        Some("string"),
        mfn1(|s: RcStr, needle: RcStr| -> Result<i32, String> {
            Ok(match s.find(&*needle) {
                Some(pos) => pos as i32,
                None => -1,
            })
        }),
    );

    vm.register_method(
        ValueTag::Str,
        "parse",
        Some("string"),
        mfn0(|s: RcStr| -> Result<Value, String> {
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
        }),
    );
}
