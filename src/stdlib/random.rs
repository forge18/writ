use std::cell::RefCell;
use std::rc::Rc;

use rand::Rng;
use crate::vm::binding::{fn0, fn1, fn2};
use crate::vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn_in_module(
        "random",
        "random",
        fn0(|| -> Result<f64, String> { Ok(rand::thread_rng().r#gen()) }),
    );

    vm.register_fn_in_module(
        "randomInt",
        "random",
        fn2(|min: i64, max: i64| -> Result<Value, String> {
            let val = rand::thread_rng().gen_range(min..=max);
            if let Ok(v) = i32::try_from(val) {
                Ok(Value::I32(v))
            } else {
                Ok(Value::I64(val))
            }
        }),
    );

    vm.register_fn_in_module(
        "randomFloat",
        "random",
        fn2(|min: f64, max: f64| -> Result<f64, String> {
            Ok(rand::thread_rng().gen_range(min..max))
        }),
    );

    vm.register_fn_in_module(
        "shuffle",
        "random",
        fn1(|arr: Rc<RefCell<Vec<Value>>>| -> Result<(), String> {
            let mut items = arr.borrow_mut();
            let len = items.len();
            if len > 1 {
                let mut rng = rand::thread_rng();
                for i in (1..len).rev() {
                    let j = rng.gen_range(0..=i);
                    items.swap(i, j);
                }
            }
            Ok(())
        }),
    );
}
