use std::time::{SystemTime, UNIX_EPOCH};

use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn_in_module("now", "time", 0, |_args| {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        Ok(Value::F64(timestamp))
    });

    vm.register_fn_in_module("elapsed", "time", 1, |args| {
        let start = match &args[0] {
            v @ (Value::F32(_) | Value::F64(_)) => v.as_f64(),
            _ => return Err("elapsed expects a timestamp number".to_string()),
        };
        let current = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        Ok(Value::F64(current - start))
    });
}
