use std::rc::Rc;

use crate::vm::binding::{fn1, fn2};
use crate::vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn(
        "print",
        fn1(|v: Value| -> Result<(), String> {
            let output = match &v {
                Value::Str(s) => (**s).clone(),
                other => other.to_string(),
            };
            println!("{output}");
            Ok(())
        }),
    );

    vm.register_fn(
        "assert",
        fn2(|condition: bool, message: Value| -> Result<(), String> {
            if !condition {
                let msg = match &message {
                    Value::Str(s) => (**s).clone(),
                    other => other.to_string(),
                };
                return Err(format!("assertion failed: {msg}"));
            }
            Ok(())
        }),
    );

    vm.register_fn(
        "type",
        fn1(|v: Value| -> Result<Value, String> { Ok(Value::Str(Rc::new(v.type_name_owned()))) }),
    );
}
