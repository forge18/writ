//! Writ standard library — registers native functions and methods into the VM.

mod array;
mod basic;
mod color;
mod dict;
mod input;
mod interpolation;
mod io;
mod math;
pub(crate) mod matrix;
mod noise;
mod quaternion;
mod random;
mod rect;
mod reflect;
mod regex;
mod string;
mod time;
mod timer;
mod transform;
mod tween;
pub(crate) mod vector;

use crate::vm::VM;

/// Registers all standard library modules into the VM.
pub fn register_all(vm: &mut VM) {
    basic::register(vm);
    math::register(vm);
    string::register(vm);
    array::register(vm);
    dict::register(vm);
    io::register(vm);
    time::register(vm);
    random::register(vm);
    reflect::register(vm);
    vector::register(vm);
    matrix::register(vm);
    quaternion::register(vm);
    transform::register(vm);
    rect::register(vm);
    color::register(vm);
    interpolation::register(vm);
    noise::register(vm);
    tween::register(vm);
    timer::register(vm);
    input::register(vm);
    regex::register(vm);
}

/// Registers all standard library modules except the ones listed in `excluded`.
///
/// Module names: `"basic"`, `"math"`, `"string"`, `"array"`, `"dictionary"`,
/// `"io"`, `"time"`, `"random"`, `"reflection"`, `"vector"`, `"matrix"`,
/// `"quaternion"`, `"transform"`, `"rectangle"`, `"color"`, `"interpolation"`,
/// `"noise"`, `"tween"`, `"timer"`, `"input"`, `"regex"`.
pub fn register_except(vm: &mut VM, excluded: &[&str]) {
    type ModuleEntry = (&'static str, fn(&mut VM));
    let modules: &[ModuleEntry] = &[
        ("basic", basic::register),
        ("math", math::register),
        ("string", string::register),
        ("array", array::register),
        ("dictionary", dict::register),
        ("io", io::register),
        ("time", time::register),
        ("random", random::register),
        ("reflection", reflect::register),
        ("vector", vector::register),
        ("matrix", matrix::register),
        ("quaternion", quaternion::register),
        ("transform", transform::register),
        ("rectangle", rect::register),
        ("color", color::register),
        ("interpolation", interpolation::register),
        ("noise", noise::register),
        ("tween", tween::register),
        ("timer", timer::register),
        ("input", input::register),
        ("regex", regex::register),
    ];
    for (name, register_fn) in modules {
        if !excluded.contains(name) {
            register_fn(vm);
        }
    }
}
