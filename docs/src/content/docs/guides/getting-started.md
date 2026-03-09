---
title: Getting Started
description: Add Writ to your Rust project and run your first script.
---

Writ is an embedded scripting language for Rust. Add it as a dependency, write a `.writ` script, and load it from your application.

## Install

```toml
[dependencies]
writ = "0.1"
```

## Write a script

Create `scripts/hello.writ`:

```writ
func greet(name: string) -> string {
    return "Hello, " + name + "!"
}

print(greet("World"))
```

## Create a VM

Every Writ program runs inside a `Writ` instance. Create one — the standard library is loaded automatically:

```rust
use writ::Writ;

let mut vm = Writ::new();
```

## Load and run it

`load()` compiles the file, executes top-level code, and makes functions available for `call()`:

```rust
vm.load("scripts/hello.writ").unwrap();

// Call a script function from Rust.
let result = vm.call("greet", &[writ::Value::Str("Rust".into())]).unwrap();
println!("{result}"); // Hello, Rust!
```

## Give scripts access to your application

Register Rust functions so scripts can call back into your code:

```rust
use writ::{Type, fn2};

vm.register_host_fn(
    "damage",
    vec![Type::Float, Type::Float],
    Type::Float,
    fn2(|hp: f64, amount: f64| -> Result<f64, String> {
        Ok((hp - amount).max(0.0))
    }),
);
```

Now scripts can call `damage()` as if it were a built-in:

```writ
let remaining = damage(100.0, 35.0)
print(remaining)   // 65.0
```

The type checker validates calls at compile time — wrong argument types or counts produce errors before the script runs.

## Build something real

Here's a more complete example — a script with a class, and Rust code that loads and drives it:

```writ
// scripts/combat.writ

class Fighter {
    public name: string
    public health: float = 100.0

    public func takeDamage(amount: float) {
        health -= amount
        if health <= 0 {
            print(name + " was defeated!")
        }
    }

    public func isAlive() -> bool {
        return health > 0
    }
}

func createFighter(name: string) -> Fighter {
    return Fighter(name: name)
}
```

```rust
use writ::{Value, Writ};

fn main() {
    let mut vm = Writ::new();
    vm.load("scripts/combat.writ").unwrap();

    let fighter = vm.call("createFighter", &[Value::Str("Hero".into())]).unwrap();
    println!("{fighter}"); // Fighter { name: "Hero", health: 100.0 }
}
```

## Next steps

- **[Language Fundamentals](/writ/language/fundamentals/)** — variables, types, operators, strings
- **[Classes & Structs](/writ/language/classes-and-structs/)** — custom types, inheritance, traits
- **[Modules](/writ/language/modules/)** — split scripts across files with import/export
- **[Embedding Guide](/writ/advanced/embedding/)** — host types, sandboxing, hot reload
- **[Standard Library](/writ/stdlib/core/)** — what's available out of the box
