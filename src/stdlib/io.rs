use std::rc::Rc;

use crate::vm::binding::{fn0, fn1, fn2};
use crate::vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn_in_module(
        "readFile",
        "io",
        fn1(|path: String| -> Result<String, String> {
            std::fs::read_to_string(&path).map_err(|e| format!("readFile failed: {e}"))
        }),
    );

    vm.register_fn_in_module(
        "writeFile",
        "io",
        fn2(|path: String, content: String| -> Result<(), String> {
            std::fs::write(&path, &content).map_err(|e| format!("writeFile failed: {e}"))
        }),
    );

    vm.register_fn_in_module(
        "readLine",
        "io",
        fn0(|| -> Result<Value, String> {
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| format!("readLine failed: {e}"))?;
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            Ok(Value::Str(Rc::new(line)))
        }),
    );

    vm.register_fn_in_module(
        "fileExists",
        "io",
        fn1(|path: String| -> Result<bool, String> { Ok(std::path::Path::new(&path).exists()) }),
    );
}
