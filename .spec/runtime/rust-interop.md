# Rust Interop

> **Crate:** `writ-vm` | **Status:** Draft

## 1. Purpose

This spec defines how Writ scripts interface with Rust host code — type mapping, inheritance compilation, the binding model, and sandboxing enforcement.

## 2. Dependencies

| Depends On                                   | Relationship                                         |
|----------------------------------------------|------------------------------------------------------|
| [vm.md](vm.md)                               | Interop is implemented inside the VM's binding layer |
| [type-system.md](../language/type-system.md) | Script types must be declared to the type checker    |

---

## 3. Type Mapping

### 3.1 Primitives

Numeric types use smart width promotion — they start at the narrowest Rust representation and promote automatically when needed. Near-zero conversion cost at the FFI boundary.

| Script Type | Rust Type                   |
|-------------|-----------------------------|
| `int`       | `i32` (promotes to `i64`)   |
| `float`     | `f32` (promotes to `f64`)   |
| `bool`      | `bool`                      |
| `string`    | `String`                    |

When a host function expects a specific width or signedness (e.g., `u32`, `i64`, `f32`), the binding layer validates at the FFI boundary:

- Exact width match → zero conversion cost
- Wider-to-narrower (e.g., `i64` value to `i32` param) → range check, essentially free
- Signed-to-unsigned (e.g., `int` value to `u32` param) → non-negative check, essentially free

### 3.2 Collections

| Script Type        | Rust Type           |
|--------------------|---------------------|
| `Array<T>`         | `Vec<T>`            |
| `Dictionary<K, V>` | `HashMap<K, V>`     |
| `Optional<T>`      | `Option<T>`         |
| `Result<T>`        | `Result<T, String>` |

### 3.3 User-Defined Types

User-defined types compile to first-class Rust types, not VM-managed wrappers. They have identical memory layout to their Rust counterparts and cross the FFI boundary without conversion.

---

## 4. Inheritance Compilation

Single inheritance via `extends` compiles to **composition with `Deref`** in Rust. The compiler generates this automatically — neither the scripter nor the host developer sees the implementation.

```writ
// Writ source
class Player extends Entity {
    public health: float
}
```

```rust
// Generated Rust
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

This means `player.name` automatically resolves to `player.base.name` when `name` is a field on `Entity`. No boilerplate required from the host developer.

---

## 5. Trait Compilation

Script traits compile to Rust traits. Default implementations are preserved. A class that declares `with TraitName` generates a Rust `impl TraitName for ClassName` block.

```writ
// Writ source
trait Damageable {
    func takeDamage(amount: float)
    func die() {
        print("Entity died")
    }
}

class Player with Damageable {
    func takeDamage(amount: float) {
        health -= amount
    }
    // die() uses default implementation
}
```

```rust
// Generated Rust
trait Damageable {
    fn take_damage(&mut self, amount: f32);
    fn die(&mut self) {
        println!("Entity died");
    }
}

impl Damageable for Player {
    fn take_damage(&mut self, amount: f32) {
        self.health -= amount;
    }
    // die() uses default
}
```

---

## 6. Binding Model

The VM starts with zero external access. The host explicitly opts in to what scripts can use.

```rust
let vm = VM::new()
    .register_type::<Player>()
    .register_type::<Vector2>()
    .register_fn("move", player_move)
    .register_fn("spawn", entity_spawn)
    .register_fn("on_update", on_update)
    .register_fn("on_collision", on_collision)
```

### 6.1 register_type\<T\>()

Exposes a Rust type to the script environment. The script can:

- Declare variables of that type
- Call methods registered on that type
- Pass instances as function arguments and return values

The type must implement the `WritType` trait (auto-derivable via `#[derive(WritType)]`).

### 6.2 register_fn(name, fn)

Registers a Rust function callable from scripts. The function's parameter and return types must be expressible in Writ's type system.

### 6.3 disable_module(name)

Prevents scripts from calling any function in the named standard library module. Used to restrict untrusted mod scripts.

```rust
let mod_vm = VM::new()
    .disable_module("io")
    .instruction_limit(100_000)
```

---

## 7. Sandboxing Enforcement

The VM actively enforces the registered API surface. A script that attempts to call an unregistered function produces a compile-time error — the type checker catches this before bytecode is emitted.

At runtime, the binding layer validates all type casts when a host-owned value is passed to a script. Invalid casts produce a runtime error with a full stack trace.

---

## 8. Edge Cases

1. **Given** a Rust type is registered in one VM instance but not another, **then** scripts in the second VM cannot declare variables of that type — compile error.
2. **Given** a registered Rust function returns `Result<T, String>`, **then** the script sees `Result<T>` and can use `?`, `is Success`, and `is Error` normally.
3. **Given** a host destroys a Rust object while a script holds a reference to it, **then** the script's reference becomes a dangling reference. The host is responsible for ensuring object lifetimes exceed any script references.

---

## 9. Revision History

| Date       | Change                                                          |
|------------|-----------------------------------------------------------------|
| 2026-03-03 | Simplified numeric types to `int`/`float` with smart promotion  |
| 2026-03-02 | Initial draft                                                   |
