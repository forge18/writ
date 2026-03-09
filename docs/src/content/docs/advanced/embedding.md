---
title: Embedding Guide
description: Expose Rust types to scripts, sandbox the VM, and integrate hot reload.
---

This guide covers everything you need to deeply integrate Writ into a Rust host application.

## The Writ struct

`Writ` is the single entry point. It owns the VM, type checker, and standard library. Create one instance per scripting context — for example, one per game world or one per plugin sandbox.

```rust
use writ::Writ;

let mut vm = Writ::new(); // stdlib pre-loaded
```

## Exposing functions to scripts

### With type information (preferred)

`register_host_fn` registers in both the VM and the type checker. Scripts get proper compile-time errors for wrong argument types.

```rust
use writ::{Type, fn2};

vm.register_host_fn(
    "damage",
    vec![Type::Float, Type::Float],  // param types
    Type::Float,                      // return type
    fn2(|hp: f64, amount: f64| -> Result<f64, String> {
        Ok((hp - amount).max(0.0))
    }),
);
```

### Without type information

Use `register_host_fn_untyped` for dynamic dispatch or FFI wrappers where types can't be expressed statically. The type checker allows any arguments; all other checking still runs.

```rust
use writ::fn1;

vm.register_host_fn_untyped("dispatch", fn1(|arg: writ::Value| -> Result<writ::Value, String> {
    // handle arg dynamically
    Ok(writ::Value::Null)
}));
```

### Helper macros

Writ provides `fn0`–`fn3` and `mfn0`–`mfn3` (method variants) to wrap typed Rust closures:

```rust
use writ::{fn0, fn1, fn2, fn3};

vm.register_fn("now",   fn0(|| -> Result<f64, String> { Ok(0.0) }));
vm.register_fn("abs",   fn1(|x: f64| -> Result<f64, String> { Ok(x.abs()) }));
vm.register_fn("clamp", fn3(|v: f64, lo: f64, hi: f64| -> Result<f64, String> {
    Ok(v.clamp(lo, hi))
}));
```

## Exposing Rust types

Implement `WritObject` on any Rust struct to make it available as a script value.

```rust
use writ::{WritObject, Value};
use std::any::Any;

struct Player {
    name: String,
    health: f32,
}

impl WritObject for Player {
    fn type_name(&self) -> &str { "Player" }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "name"   => Ok(Value::Str(self.name.clone().into())),
            "health" => Ok(Value::F32(self.health)),
            _ => Err(format!("Player has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "health" => {
                self.health = value.as_f64() as f32;
                Ok(())
            }
            _ => Err(format!("Player has no settable field '{name}'")),
        }
    }

    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        match name {
            "greet" => Ok(Value::Str(format!("I'm {}!", self.name).into())),
            _ => Err(format!("Player has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
```

Then register a factory so scripts can construct instances with `Player(...)`:

```rust
vm.register_type("Player", |args| {
    let name = args.first()
        .and_then(|v| v.as_str_opt())
        .unwrap_or("Unknown")
        .to_string();
    Ok(Box::new(Player { name, health: 100.0 }))
});
```

## Globals

Expose a constant or pre-built value that scripts can read:

```rust
vm.register_global("MAX_PLAYERS", writ::Value::I32(64));
vm.register_global("VERSION", writ::Value::Str("1.0".into()));
```

## Loading and calling

```rust
// Load a file — top-level runs once, functions persist
vm.load("scripts/combat.writ").unwrap();

// Call a named function
let result = vm.call("calculateDamage", &[
    Value::F32(50.0),  // attacker power
    Value::F32(10.0),  // defender armor
]).unwrap();
```

## Coroutine scheduler

If scripts use coroutines (`yield`, `start`), call `tick` every frame with the elapsed time:

```rust
fn update(vm: &mut Writ, delta_seconds: f64) {
    vm.tick(delta_seconds).unwrap();
}
```

When a game object is destroyed, cancel its coroutines to avoid dangling execution:

```rust
fn on_destroy(vm: &mut Writ, entity_id: u64) {
    vm.cancel_coroutines_for_owner(entity_id);
}
```

## Error handling

All pipeline stages return `WritError`, which wraps each stage's specific error type:

```rust
match vm.run(source) {
    Ok(value) => { /* use value */ }
    Err(writ::WritError::Type(e)) => eprintln!("Type error: {e}"),
    Err(writ::WritError::Runtime(e)) => {
        // Display includes the full stack trace automatically
        eprintln!("{e}");
    }
    Err(e) => eprintln!("Error: {e}"),
}
```

Runtime errors include a full stack trace with file, line, and function name at each frame. The stack trace is printed automatically via the `Display` implementation. Individual frames are also accessible via `e.trace.frames`.
