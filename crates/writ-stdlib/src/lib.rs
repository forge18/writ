//! Writ standard library — registers native functions and methods into the VM.

mod array;
mod basic;
mod dict;
mod io;
mod math;
mod random;
mod reflect;
mod string;
mod time;

use writ_vm::VM;

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
}

/// Registers all standard library modules except the ones listed in `excluded`.
///
/// Module names: `"basic"`, `"math"`, `"string"`, `"array"`, `"dictionary"`,
/// `"io"`, `"time"`, `"random"`, `"reflection"`.
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
    ];
    for (name, register_fn) in modules {
        if !excluded.contains(name) {
            register_fn(vm);
        }
    }
}
