use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::vm::binding::{mfn0, mfn1};
use crate::vm::{VM, Value, ValueTag};

type Dict = Rc<RefCell<HashMap<String, Value>>>;

pub fn register(vm: &mut VM) {
    vm.register_method(
        ValueTag::Dict,
        "keys",
        Some("dictionary"),
        mfn0(|dict: Dict| -> Result<Value, String> {
            let keys: Vec<Value> = dict
                .borrow()
                .keys()
                .map(|k| Value::Str(Rc::from(k.as_str())))
                .collect();
            Ok(Value::Array(Rc::new(RefCell::new(keys))))
        }),
    );

    vm.register_method(
        ValueTag::Dict,
        "values",
        Some("dictionary"),
        mfn0(|dict: Dict| -> Result<Value, String> {
            let values: Vec<Value> = dict.borrow().values().cloned().collect();
            Ok(Value::Array(Rc::new(RefCell::new(values))))
        }),
    );

    vm.register_method(
        ValueTag::Dict,
        "has",
        Some("dictionary"),
        mfn1(|dict: Dict, key: Rc<str>| -> Result<bool, String> {
            Ok(dict.borrow().contains_key(&*key))
        }),
    );

    vm.register_method(
        ValueTag::Dict,
        "remove",
        Some("dictionary"),
        mfn1(|dict: Dict, key: Rc<str>| -> Result<Value, String> {
            Ok(dict
                .borrow_mut()
                .remove(&*key)
                .unwrap_or(Value::Null))
        }),
    );

    vm.register_method(
        ValueTag::Dict,
        "len",
        Some("dictionary"),
        mfn0(|dict: Dict| -> Result<i32, String> { Ok(dict.borrow().len() as i32) }),
    );

    vm.register_method(
        ValueTag::Dict,
        "isEmpty",
        Some("dictionary"),
        mfn0(|dict: Dict| -> Result<bool, String> { Ok(dict.borrow().is_empty()) }),
    );

    vm.register_method(
        ValueTag::Dict,
        "merge",
        Some("dictionary"),
        mfn1(|dict: Dict, other: Dict| -> Result<(), String> {
            let other_entries = other.borrow();
            let mut target = dict.borrow_mut();
            for (k, v) in other_entries.iter() {
                target.insert(k.clone(), v.clone());
            }
            Ok(())
        }),
    );
}
