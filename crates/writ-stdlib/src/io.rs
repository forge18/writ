use std::rc::Rc;

use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn_in_module("readFile", "io", 1, |args| {
        let path = match &args[0] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("readFile expects a string path".to_string()),
        };
        std::fs::read_to_string(&path)
            .map(|content| Value::Str(Rc::new(content)))
            .map_err(|e| format!("readFile failed: {e}"))
    });

    vm.register_fn_in_module("writeFile", "io", 2, |args| {
        let path = match &args[0] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("writeFile expects a string path".to_string()),
        };
        let content = match &args[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("writeFile expects string content".to_string()),
        };
        std::fs::write(&path, &content).map_err(|e| format!("writeFile failed: {e}"))?;
        Ok(Value::Null)
    });

    vm.register_fn_in_module("readLine", "io", 0, |_args| {
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| format!("readLine failed: {e}"))?;
        // Trim trailing newline
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(Value::Str(Rc::new(line)))
    });

    vm.register_fn_in_module("fileExists", "io", 1, |args| {
        let path = match &args[0] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("fileExists expects a string path".to_string()),
        };
        Ok(Value::Bool(std::path::Path::new(&path).exists()))
    });
}
