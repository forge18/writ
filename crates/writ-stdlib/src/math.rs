use writ_vm::binding::{fn1, fn2, fn3};
use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_global("PI", Value::F64(std::f64::consts::PI));
    vm.register_global("TAU", Value::F64(std::f64::consts::TAU));
    vm.register_global("INFINITY", Value::F64(f64::INFINITY));

    vm.register_fn_in_module(
        "abs",
        "math",
        fn1(|v: Value| -> Result<Value, String> {
            match v {
                Value::I32(n) => Ok(Value::I32(n.abs())),
                Value::I64(n) => Ok(Value::I64(n.abs())),
                Value::F32(f) => Ok(Value::F32(f.abs())),
                Value::F64(f) => Ok(Value::F64(f.abs())),
                other => Err(format!("abs expects a number, got {}", other.type_name())),
            }
        }),
    );

    vm.register_fn_in_module(
        "ceil",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.ceil()) }),
    );
    vm.register_fn_in_module(
        "floor",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.floor()) }),
    );
    vm.register_fn_in_module(
        "round",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.round()) }),
    );
    vm.register_fn_in_module(
        "sqrt",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.sqrt()) }),
    );
    vm.register_fn_in_module(
        "sin",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.sin()) }),
    );
    vm.register_fn_in_module(
        "cos",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.cos()) }),
    );
    vm.register_fn_in_module(
        "tan",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.tan()) }),
    );
    vm.register_fn_in_module(
        "log",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.ln()) }),
    );
    vm.register_fn_in_module(
        "exp",
        "math",
        fn1(|x: f64| -> Result<f64, String> { Ok(x.exp()) }),
    );

    vm.register_fn_in_module(
        "min",
        "math",
        fn2(|a: f64, b: f64| -> Result<f64, String> { Ok(a.min(b)) }),
    );
    vm.register_fn_in_module(
        "max",
        "math",
        fn2(|a: f64, b: f64| -> Result<f64, String> { Ok(a.max(b)) }),
    );
    vm.register_fn_in_module(
        "pow",
        "math",
        fn2(|base: f64, exp: f64| -> Result<f64, String> { Ok(base.powf(exp)) }),
    );

    vm.register_fn_in_module(
        "clamp",
        "math",
        fn3(
            |x: f64, min_val: f64, max_val: f64| -> Result<f64, String> {
                Ok(x.clamp(min_val, max_val))
            },
        ),
    );
}
