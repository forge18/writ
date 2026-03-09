---
title: Runtime & Memory
description: Execution model, bytecode pipeline, memory management, and Writ-to-Rust type mapping.
---

## Execution model

Writ is a **bytecode VM** — scripts are compiled to bytecode at load time and executed on a lightweight VM embedded in the Rust host.

### Pipeline

```
Source → Lexer → Parser → AST → Type Checker → Bytecode → VM
```

### Key properties

- Scripts loaded and executed at runtime — no ahead-of-time compilation required
- Static typing means all type checking happens before bytecode is emitted — no runtime type checks
- Near-zero marshalling cost — script types map directly to Rust types in memory
- VM starts with no external access — host explicitly registers what scripts can use

---

## Memory management

Three-tier model — no garbage collector, no GC pauses.

### Stack allocation

- Temporaries and local variables
- Automatic, zero overhead
- Freed when function returns

### Reference counting

- Heap-allocated objects — collections, script-created instances
- Freed when no more references exist
- Predictable, no pause spikes
- Circular references handled via weak references

### Host ownership

- Entity-bound script objects owned by Rust host
- Rust's ownership system manages lifetime
- When entity is destroyed, script goes with it
- Zero VM overhead for host-owned objects

---

## Type mapping

All primitive types map directly to Rust equivalents with identical memory layout — no conversion cost at the boundary.

| Script type          | Rust type                |
|----------------------|--------------------------|
| `int`                | `i32` / `i64`            |
| `float`              | `f32` / `f64`            |
| `bool`               | `bool`                   |
| `string`             | `String`                 |
| `Array<T>`           | `Vec<T>`                 |
| `Dictionary<K, V>`   | `HashMap<K, V>`          |
| `Optional<T>`        | `Option<T>`              |
| `Result<T>`          | `Result<T, String>`      |

User-defined types compile to first-class Rust types, not VM-managed wrappers.

### Inheritance compilation

Single inheritance via `extends` compiles to **composition with `Deref`** in Rust. The compiler generates this automatically — neither the scripter nor the host developer sees the implementation.

```rust
// Generated from: class Player extends Entity { pub health: float }
pub struct Player {
    base: Entity,
    pub health: f32,
}

impl std::ops::Deref for Player {
    type Target = Entity;
    fn deref(&self) -> &Entity { &self.base }
}

impl std::ops::DerefMut for Player {
    fn deref_mut(&mut self) -> &mut Entity { &mut self.base }
}
```

### Traits

Script traits compile to Rust traits. Default implementations are preserved.
