# Writ

Native speed. Familiar syntax. Built for games.

Writ is a statically typed scripting language designed for game developers. It embeds directly into Rust with near-zero interop cost — no marshalling, no runtime overhead, no fighting the borrow checker. Familiar to anyone who has written GDScript, C#, or TypeScript. Fast enough to run in a real-time game loop. Write game logic in Writ. Let Rust handle the rest.

```writ
func greet(name: string) -> string {
    return "Hello, " .. name .. "!"
}
```

---

## Features

- **Static typing** — all types checked before execution, no runtime surprises
- **Clean Rust interop** — host types map directly to Rust memory; no wrapper overhead
- **Hot reload** — swap function bytecode at runtime without losing VM state
- **Capability-based sandboxing** — VM starts with zero access; you register exactly what scripts can call
- **Coroutines** — GDScript-style `yield` with structured lifetime management
- **Standard library** — math, string, array, dict, regex, noise, tweening, and more

---

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
writ = "0.1"
```

Or from git:

```toml
[dependencies]
writ = { git = "https://github.com/forge18/writ" }
```

---

## Quick Start

```rust
use writ::Writ;

fn main() {
    let mut vm = Writ::new();

    // Run a script inline
    let result = vm.run("return 1 + 2").unwrap();
    println!("{result}"); // 3

    // Register a host function
    vm.register_host_fn(
        "damage",
        vec![writ::Type::Float, writ::Type::Float],
        writ::Type::Float,
        writ::fn2(|hp: f64, amount: f64| -> Result<f64, String> {
            Ok((hp - amount).max(0.0))
        }),
    );

    // Call a function defined in a script
    vm.run("func double(n: int) -> int { return n * 2 }").unwrap();
    let result = vm.call("double", &[writ::Value::I32(21)]).unwrap();
    println!("{result}"); // 42
}
```

---

## Embedding

### Registering host types

Implement `WritObject` on any Rust struct to expose it to scripts:

```rust
use writ::{WritObject, Value};

struct Player { name: String, health: f32 }

impl WritObject for Player {
    fn type_name(&self) -> &str { "Player" }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "name"   => Ok(Value::Str(self.name.clone().into())),
            "health" => Ok(Value::F32(self.health)),
            _        => Err(format!("no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "health" => { self.health = value.as_f64() as f32; Ok(()) }
            _        => Err(format!("no field '{name}'")),
        }
    }

    fn call_method(&mut self, name: &str, _args: &[Value]) -> Result<Value, String> {
        match name {
            "greet" => Ok(Value::Str(format!("I'm {}!", self.name).into())),
            _       => Err(format!("no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}
```

Register a factory so scripts can construct instances:

```rust
vm.register_type("Player", |args| {
    let name = args[0].as_str().to_string();
    Ok(Box::new(Player { name, health: 100.0 }))
});
```

### Loading files

```rust
// Load a file — top-level code runs, functions become callable
vm.load("scripts/combat.writ").unwrap();

// Call a function defined in the file
let result = vm.call("calculateDamage", &[Value::F32(50.0)]).unwrap();

// Hot-reload during development — swaps bytecode, preserves VM state
vm.reload("scripts/combat.writ").unwrap();
```

### Coroutines

```rust
// Tick the coroutine scheduler each frame
fn update(vm: &mut Writ, delta: f64) {
    vm.tick(delta).unwrap();
}

// When a game object is destroyed, clean up its coroutines
fn on_destroy(vm: &mut Writ, entity_id: u64) {
    vm.cancel_coroutines_for_owner(entity_id);
}
```

### Sandboxing

```rust
let mut vm = Writ::new();

// Limit script execution time
vm.set_instruction_limit(1_000_000);

// Block entire stdlib modules
vm.disable_module("io");
vm.disable_module("noise");
```

---

## Language

### Variables

```writ
const MAX_HEALTH = 100.0   // compile-time constant
let name = "Hero"          // immutable at runtime
var health = 100.0         // mutable
```

### Functions

```writ
func takeDamage(amount: float) -> float {
    return (health - amount).max(0.0)
}

// Lambda
let double = (x: int) => x * 2
```

### Structs

Value types, copied on assignment.

```writ
struct Vec2 {
    x: float
    y: float

    func length() -> float {
        return (x * x + y * y).sqrt()
    }
}

let v = Vec2(x: 3.0, y: 4.0)
print(v.length()) // 5.0
```

### Classes

Reference types with single inheritance and multiple traits.

```writ
class Entity {
    public id: int
    public name: string
}

class Player extends Entity {
    public health: float = 100.0
        set(value) { field = clamp(value, 0.0, 100.0) }

    public func takeDamage(amount: float) {
        health -= amount
    }
}
```

### Control flow

```writ
// if / else
if health <= 0 {
    die()
} else if health < 20 {
    playLowHealthSound()
}

// for loop
for item in inventory {
    print(item.name)
}

// while
while health > 0 {
    tick()
}

// Pattern matching
when result {
    is Success(value) => print(value)
    is Error(msg)     => print("Error: " .. msg)
}
```

### Optionals and Results

```writ
let target: Optional<Player>  // may be absent

// Safe access
let hp = target?.health ?? 0.0

// Result propagation
func load(path: string) -> Result<Data> {
    let raw = readFile(path)?   // propagates error
    return Success(parse(raw))
}
```

### Coroutines

```writ
func countdown(from: int) {
    var n = from
    while n > 0 {
        print(n)
        yield seconds(1.0)
        n -= 1
    }
}

start countdown(from: 3)  // non-blocking, runs across ticks
```

---

## Optional Features

```toml
[dependencies]
writ = { version = "0.1", features = ["debug-hooks"] }
```

| Feature | Description |
|---|---|
| `debug-hooks` | Breakpoints, line hooks, call/return hooks |
| `mobile-aosoa` | AoSoA memory layout for bulk entity operations |

---

## License

MIT
