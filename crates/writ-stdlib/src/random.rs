use rand::Rng;

use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn_in_module("random", "random", 0, |_args| {
        let val: f64 = rand::thread_rng().r#gen();
        Ok(Value::F64(val))
    });

    vm.register_fn_in_module("randomInt", "random", 2, |args| {
        let min = match &args[0] {
            v @ (Value::I32(_) | Value::I64(_)) => v.as_i64(),
            _ => return Err("randomInt expects integer min".to_string()),
        };
        let max = match &args[1] {
            v @ (Value::I32(_) | Value::I64(_)) => v.as_i64(),
            _ => return Err("randomInt expects integer max".to_string()),
        };
        let val = rand::thread_rng().gen_range(min..=max);
        // Use i32 if result fits, otherwise i64
        if let Ok(v) = i32::try_from(val) {
            Ok(Value::I32(v))
        } else {
            Ok(Value::I64(val))
        }
    });

    vm.register_fn_in_module("randomFloat", "random", 2, |args| {
        let min = match &args[0] {
            v @ (Value::F32(_) | Value::F64(_)) => v.as_f64(),
            v @ (Value::I32(_) | Value::I64(_)) => v.as_i64() as f64,
            _ => return Err("randomFloat expects a number for min".to_string()),
        };
        let max = match &args[1] {
            v @ (Value::F32(_) | Value::F64(_)) => v.as_f64(),
            v @ (Value::I32(_) | Value::I64(_)) => v.as_i64() as f64,
            _ => return Err("randomFloat expects a number for max".to_string()),
        };
        let val = rand::thread_rng().gen_range(min..max);
        Ok(Value::F64(val))
    });

    vm.register_fn_in_module("shuffle", "random", 1, |args| {
        let arr = match &args[0] {
            Value::Array(a) => a,
            _ => return Err("shuffle expects an array".to_string()),
        };
        let mut items = arr.borrow_mut();
        let len = items.len();
        if len > 1 {
            let mut rng = rand::thread_rng();
            for i in (1..len).rev() {
                let j = rng.gen_range(0..=i);
                items.swap(i, j);
            }
        }
        Ok(Value::Null)
    });
}
