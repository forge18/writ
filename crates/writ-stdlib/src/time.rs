use std::time::{SystemTime, UNIX_EPOCH};

use writ_vm::VM;
use writ_vm::binding::{fn0, fn1};

pub fn register(vm: &mut VM) {
    vm.register_fn_in_module(
        "now",
        "time",
        fn0(|| -> Result<f64, String> {
            Ok(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64())
        }),
    );

    vm.register_fn_in_module(
        "elapsed",
        "time",
        fn1(|start: f64| -> Result<f64, String> {
            let current = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            Ok(current - start)
        }),
    );
}
