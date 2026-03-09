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

---

## Coroutine scheduler

If scripts use coroutines (`yield`, `start`), register a tick source so the VM knows how to measure time. Coroutines then advance automatically whenever you call into the VM.

### Tick source (recommended)

Register once during setup:

```rust
// Game engine — use your engine's delta time
vm.set_tick_source(|| engine.delta_time());
```

After this, every `call()` or `load()` auto-ticks coroutines before executing. No per-frame tick call needed.

The callback controls what "time" means to coroutines:

- Return `0.0` to freeze coroutines (pause)
- Return `delta * 0.5` for slow motion
- Return a fixed value for deterministic playback

For non-game applications, use wall-clock time:

```rust
vm.use_wall_clock();
```

### Manual tick (advanced)

For direct control over timing, skip `set_tick_source` and call `tick()` explicitly each frame:

```rust
vm.tick(delta_seconds).unwrap();
```

### Structured lifetime

When a game object is destroyed, cancel its coroutines to avoid dangling execution:

```rust
fn on_destroy(vm: &mut Writ, entity_id: u64) {
    vm.cancel_coroutines_for_owner(entity_id);
}
```

Cancellation propagates to child coroutines automatically. See [Coroutines](/writ/language/coroutines/) for the script-side API.
