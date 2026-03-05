use writ_vm::{VM, Value};

/// Extracts a float64 from a Value, promoting integers to floats.
fn to_f64(v: &Value) -> Result<f64, String> {
    match v {
        v @ (Value::F32(_) | Value::F64(_)) => Ok(v.as_f64()),
        v @ (Value::I32(_) | Value::I64(_)) => Ok(v.as_i64() as f64),
        _ => Err(format!("expected a number, got {}", v.type_name())),
    }
}

/// Returns the result as the same numeric type as the input when possible.
fn from_f64_matching(result: f64, original: &Value) -> Value {
    match original {
        Value::F32(_) => {
            let narrow = result as f32;
            if narrow.is_finite() || !result.is_finite() {
                Value::F32(narrow)
            } else {
                Value::F64(result)
            }
        }
        Value::F64(_) => Value::F64(result),
        Value::I32(_)
            if result.fract() == 0.0 && result >= i32::MIN as f64 && result <= i32::MAX as f64 =>
        {
            Value::I32(result as i32)
        }
        Value::I64(_)
            if result.fract() == 0.0 && result >= i64::MIN as f64 && result <= i64::MAX as f64 =>
        {
            Value::I64(result as i64)
        }
        _ => Value::F64(result),
    }
}

pub fn register(vm: &mut VM) {
    // Constants — inherently f64 precision
    vm.register_global("PI", Value::F64(std::f64::consts::PI));
    vm.register_global("TAU", Value::F64(std::f64::consts::TAU));
    vm.register_global("INFINITY", Value::F64(f64::INFINITY));

    // Single-argument math functions
    vm.register_fn_in_module("abs", "math", 1, |args| match &args[0] {
        Value::I32(v) => Ok(Value::I32(v.abs())),
        Value::I64(v) => Ok(Value::I64(v.abs())),
        Value::F32(v) => Ok(Value::F32(v.abs())),
        Value::F64(v) => Ok(Value::F64(v.abs())),
        _ => Err(format!("abs expects a number, got {}", args[0].type_name())),
    });

    vm.register_fn_in_module("ceil", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(from_f64_matching(v.ceil(), &args[0]))
    });

    vm.register_fn_in_module("floor", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(from_f64_matching(v.floor(), &args[0]))
    });

    vm.register_fn_in_module("round", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(from_f64_matching(v.round(), &args[0]))
    });

    vm.register_fn_in_module("sqrt", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(Value::F64(v.sqrt()))
    });

    vm.register_fn_in_module("sin", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(Value::F64(v.sin()))
    });

    vm.register_fn_in_module("cos", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(Value::F64(v.cos()))
    });

    vm.register_fn_in_module("tan", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(Value::F64(v.tan()))
    });

    vm.register_fn_in_module("log", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(Value::F64(v.ln()))
    });

    vm.register_fn_in_module("exp", "math", 1, |args| {
        let v = to_f64(&args[0])?;
        Ok(Value::F64(v.exp()))
    });

    // Two-argument math functions
    vm.register_fn_in_module("min", "math", 2, |args| {
        let a = to_f64(&args[0])?;
        let b = to_f64(&args[1])?;
        Ok(from_f64_matching(a.min(b), &args[0]))
    });

    vm.register_fn_in_module("max", "math", 2, |args| {
        let a = to_f64(&args[0])?;
        let b = to_f64(&args[1])?;
        Ok(from_f64_matching(a.max(b), &args[0]))
    });

    vm.register_fn_in_module("pow", "math", 2, |args| {
        let base = to_f64(&args[0])?;
        let exp = to_f64(&args[1])?;
        Ok(Value::F64(base.powf(exp)))
    });

    // Three-argument: clamp
    vm.register_fn_in_module("clamp", "math", 3, |args| {
        let x = to_f64(&args[0])?;
        let min_val = to_f64(&args[1])?;
        let max_val = to_f64(&args[2])?;
        Ok(from_f64_matching(x.clamp(min_val, max_val), &args[0]))
    });
}
