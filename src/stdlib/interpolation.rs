use std::f64::consts::PI;

use crate::vm::binding::{fn1, fn3};
use crate::vm::{VM, Value};

use super::vector::extract_f64;

pub fn register(vm: &mut VM) {
    // --- Core interpolation ---

    vm.register_fn_in_module(
        "lerp",
        "interpolation",
        fn3(|a: f64, b: f64, t: f64| -> Result<f64, String> { Ok(a + (b - a) * t) }),
    );

    vm.register_fn_in_module(
        "inverseLerp",
        "interpolation",
        fn3(|a: f64, b: f64, value: f64| -> Result<f64, String> {
            if (b - a).abs() < f64::EPSILON {
                return Ok(0.0);
            }
            Ok((value - a) / (b - a))
        }),
    );

    vm.register_fn_in_module(
        "remap",
        "interpolation",
        super::matrix::RawFn(std::rc::Rc::new(|args: &[Value]| {
            let value = extract_f64(args.first().ok_or("missing value")?, "value")?;
            let from_min = extract_f64(args.get(1).ok_or("missing fromMin")?, "fromMin")?;
            let from_max = extract_f64(args.get(2).ok_or("missing fromMax")?, "fromMax")?;
            let to_min = extract_f64(args.get(3).ok_or("missing toMin")?, "toMin")?;
            let to_max = extract_f64(args.get(4).ok_or("missing toMax")?, "toMax")?;
            let t = (value - from_min) / (from_max - from_min);
            Ok(Value::F64(to_min + (to_max - to_min) * t))
        })),
    );

    vm.register_fn_in_module(
        "smoothstep",
        "interpolation",
        fn3(|a: f64, b: f64, t: f64| -> Result<f64, String> {
            let t = ((t - a) / (b - a)).clamp(0.0, 1.0);
            Ok(t * t * (3.0 - 2.0 * t))
        }),
    );

    vm.register_fn_in_module(
        "smootherstep",
        "interpolation",
        fn3(|a: f64, b: f64, t: f64| -> Result<f64, String> {
            let t = ((t - a) / (b - a)).clamp(0.0, 1.0);
            Ok(t * t * t * (t * (t * 6.0 - 15.0) + 10.0))
        }),
    );

    // --- Easing functions ---

    vm.register_fn_in_module(
        "easeInSine",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(1.0 - (t * PI / 2.0).cos()) }),
    );
    vm.register_fn_in_module(
        "easeOutSine",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok((t * PI / 2.0).sin()) }),
    );
    vm.register_fn_in_module(
        "easeInOutSine",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(-(PI * t).cos() / 2.0 + 0.5) }),
    );

    vm.register_fn_in_module(
        "easeInQuad",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(t * t) }),
    );
    vm.register_fn_in_module(
        "easeOutQuad",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(1.0 - (1.0 - t) * (1.0 - t)) }),
    );
    vm.register_fn_in_module(
        "easeInOutQuad",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> {
            Ok(if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            })
        }),
    );

    vm.register_fn_in_module(
        "easeInCubic",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(t * t * t) }),
    );
    vm.register_fn_in_module(
        "easeOutCubic",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(1.0 - (1.0 - t).powi(3)) }),
    );
    vm.register_fn_in_module(
        "easeInOutCubic",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> {
            Ok(if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            })
        }),
    );

    vm.register_fn_in_module(
        "easeInExpo",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> {
            Ok(if t == 0.0 {
                0.0
            } else {
                (2.0_f64).powf(10.0 * t - 10.0)
            })
        }),
    );
    vm.register_fn_in_module(
        "easeOutExpo",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> {
            Ok(if t == 1.0 {
                1.0
            } else {
                1.0 - (2.0_f64).powf(-10.0 * t)
            })
        }),
    );
    vm.register_fn_in_module(
        "easeInOutExpo",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> {
            Ok(if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else if t < 0.5 {
                (2.0_f64).powf(20.0 * t - 10.0) / 2.0
            } else {
                (2.0 - (2.0_f64).powf(-20.0 * t + 10.0)) / 2.0
            })
        }),
    );

    vm.register_fn_in_module(
        "easeInElastic",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> {
            let c4 = (2.0 * PI) / 3.0;
            Ok(if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                -(2.0_f64).powf(10.0 * t - 10.0) * ((10.0 * t - 10.75) * c4).sin()
            })
        }),
    );
    vm.register_fn_in_module(
        "easeOutElastic",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> {
            let c4 = (2.0 * PI) / 3.0;
            Ok(if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                (2.0_f64).powf(-10.0 * t) * ((10.0 * t - 0.75) * c4).sin() + 1.0
            })
        }),
    );

    vm.register_fn_in_module(
        "easeInBounce",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(1.0 - bounce_out(1.0 - t)) }),
    );
    vm.register_fn_in_module(
        "easeOutBounce",
        "interpolation",
        fn1(|t: f64| -> Result<f64, String> { Ok(bounce_out(t)) }),
    );
}

fn bounce_out(t: f64) -> f64 {
    let n1 = 7.5625;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        let t = t - 1.5 / d1;
        n1 * t * t + 0.75
    } else if t < 2.5 / d1 {
        let t = t - 2.25 / d1;
        n1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / d1;
        n1 * t * t + 0.984375
    }
}
