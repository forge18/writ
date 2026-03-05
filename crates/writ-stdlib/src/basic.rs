use std::rc::Rc;

use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn("print", 1, |args| {
        let output = match &args[0] {
            Value::Str(s) => (**s).clone(),
            other => other.to_string(),
        };
        println!("{output}");
        Ok(Value::Null)
    });

    vm.register_fn("assert", 2, |args| {
        let condition = match &args[0] {
            Value::Bool(b) => *b,
            _ => return Err("assert expects a boolean condition".to_string()),
        };
        if !condition {
            let message = match &args[1] {
                Value::Str(s) => (**s).clone(),
                other => other.to_string(),
            };
            return Err(format!("assertion failed: {message}"));
        }
        Ok(Value::Null)
    });

    vm.register_fn("type", 1, |args| {
        Ok(Value::Str(Rc::new(args[0].type_name_owned())))
    });
}
