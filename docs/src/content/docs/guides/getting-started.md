---
title: Getting Started
description: Install Writ and run your first script in minutes.
---

Writ is an embedded scripting language for Rust. You add it as a dependency, create a `Writ` instance, and run scripts from within your application.

## Installation

Add Writ to your `Cargo.toml`:

```toml
[dependencies]
writ = "0.1"
```

## Your first script

```rust
use writ::Writ;

fn main() {
    let mut vm = Writ::new();
    vm.run(r#"print("Hello from Writ!")"#).unwrap();
}
```

That's it. `Writ::new()` starts a VM with the standard library already loaded.

## Running a calculation

Scripts can return values back to Rust:

```rust
let mut vm = Writ::new();
let result = vm.run("return 2 + 2").unwrap();
println!("{result}"); // 4
```

## Defining and calling functions

Functions defined in one `run` call are available in subsequent calls:

```rust
vm.run("func double(n: int) -> int { return n * 2 }").unwrap();

let result = vm.call("double", &[writ::Value::I32(21)]).unwrap();
println!("{result}"); // 42
```

## Loading a file

For larger scripts, load from a file. Top-level code executes immediately; functions become callable:

```rust
vm.load("scripts/game.writ").unwrap();
vm.call("onStart", &[]).unwrap();
```

## Exposing Rust functions to scripts

Use `register_host_fn` to make Rust functions callable from scripts. This registers the function in both the VM and the type checker so scripts get proper type errors:

```rust
use writ::{Type, Value, fn2};

vm.register_host_fn(
    "clamp",
    vec![Type::Float, Type::Float, Type::Float],
    Type::Float,
    fn2(|value: f64, min: f64, max: f64| -> Result<f64, String> {
        Ok(value.clamp(min, max))
    }),
);

vm.run("let hp = clamp(150.0, 0.0, 100.0)").unwrap();
```

## Next steps

- **[Language Basics](/writ/language/basics)** — variables, functions, control flow
- **[Embedding Guide](/writ/guides/embedding)** — host types, sandboxing, hot reload, coroutines
- **[Standard Library](/writ/reference/stdlib)** — what's available to scripts out of the box
