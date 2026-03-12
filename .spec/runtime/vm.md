# VM

> **Crate:** `writ-vm` | **Status:** Draft

## 1. Purpose

This spec defines Writ's bytecode virtual machine — the execution engine that runs compiled `.writ` scripts. The VM manages the call stack, heap, coroutines, sandboxing, and all runtime state.

## 2. Dependencies

| Depends On                         | Relationship                                       |
|------------------------------------|----------------------------------------------------|
| [rust-interop.md](rust-interop.md) | Host registers types and functions into the VM     |
| [coroutines.md](coroutines.md)     | Coroutine scheduler lives inside the VM            |
| [debug.md](debug.md)               | Debug hooks and breakpoints are VM features        |
| [stdlib.md](../language/stdlib.md) | Standard library registered into the VM at startup |

---

## 3. Compilation Pipeline

```text
Source → Lexer → Parser → AST → Type Checker → Bytecode Compiler → Bytecode → VM
```

Scripts are compiled to bytecode at load time and executed by the VM. The VM never interprets source directly.

**Key properties:**

- All type checking happens before bytecode is emitted — no runtime type checks
- Script loading and execution happens at runtime — no ahead-of-time compilation required
- Near-zero marshalling cost — script types map directly to Rust types in memory
- VM starts with zero external access — host explicitly registers capabilities

---

## 4. Memory Layout

### 4.1 Stack

Temporaries and local variables live on the stack. Automatic, zero overhead, freed when the function returns.

### 4.2 Heap (Reference Counted)

Heap-allocated objects — collections, script-created class instances — use reference counting. Freed when no more references exist. Predictable, no GC pauses. Circular references handled via weak references.

### 4.3 Host Ownership

Entity-bound script objects are owned by the Rust host. Rust's ownership system manages their lifetime. When the entity is destroyed, the associated script object is freed. Zero VM overhead for host-owned objects.

### 4.4 Borrow Safety

The VM guarantees that no script operation or host binding can trigger a `RefCell` borrow panic. Internally, heap values are wrapped in `Rc<RefCell<T>>`. The VM handles two classes of aliasing:

1. **Receiver-argument aliasing** — When a method argument is the same object as the receiver (e.g., `q.dot(q)`, `dict.merge(dict)`), the VM detects the aliasing via `Rc::ptr_eq` at dispatch and clones the conflicting argument before borrowing.

2. **Pairwise argument aliasing** — When two arguments to a native function are the same object (e.g., `swap_health(player, player)`), the VM detects the aliasing via pairwise `Rc::ptr_eq` checks and clones the duplicate.

Detection is zero-cost in the common (non-aliasing) case — only pointer comparisons. Cloning only occurs when aliasing is actually detected.

As a safety net, all `borrow_mut()` calls at user-code dispatch sites use `try_borrow_mut()` so any missed case produces a `RuntimeError` instead of a process-terminating panic.

Neither script authors nor host developers need to be aware of this mechanism. It is fully automatic.

---

## 5. Execution Model

The VM processes bytecode instructions sequentially. The call stack tracks active function frames. Each frame contains:

- Local variable slots
- Operand stack
- Program counter
- Reference to the current bytecode chunk
- Source location (file + line, for stack traces)

---

## 6. Public API

```rust
let vm = VM::new()
    .register_type::<Player>()
    .register_type::<Vector2>()
    .register_fn("move", player_move)
    .register_fn("on_update", on_update)
    .disable_module("io")
    .instruction_limit(1_000_000)

// Imports in player.writ are resolved automatically from disk.
vm.load("entities/player.writ")?;
vm.call("on_update", &[delta])?;
```

---

## 7. Sandboxing

The VM exposes nothing by default. The host explicitly opts in to what scripts can use by registering types and functions. Different VM instances can have different API surfaces.

```rust
// Mod script VM — restricted surface
let mod_vm = VM::new()
    .disable_module("io")
    .instruction_limit(100_000)
    .register_fn("spawn_unit", spawn_unit)

// Core game script VM — full access
let game_vm = VM::new()
    .register_type::<Player>()
    .register_type::<Entity>()
    .register_fn("move", player_move)
```

Same language, same execution model, different API surfaces.

---

## 8. Error Handling

All VM errors include a full stack trace. See [debug.md](debug.md) for stack trace format.

| Error                                  | Behavior                                         |
|----------------------------------------|--------------------------------------------------|
| Script not found                       | `Result::Err` returned to host                   |
| Type error                             | Caught at compile time — cannot occur at runtime |
| Runtime error (division by zero, etc.) | Error with full stack trace returned to host     |
| Instruction limit exceeded             | Error returned to host, script terminated        |
| Unregistered function call             | Compile-time error — type checker catches this   |

---

## 9. Edge Cases

1. **Given** a script calls a host function that was not registered, **then** compile-time error — the type checker prevents this from reaching the VM.
2. **Given** a script's reference count reaches zero during execution, **then** the object is freed at the end of the current instruction — not mid-instruction.
3. **Given** a coroutine's owning object is destroyed mid-yield, **then** the coroutine is cancelled at the next resume attempt. See [coroutines.md](coroutines.md).
4. **Given** two VM instances share a registered Rust type, **then** each VM manages its own set of instances — no shared state between VMs.

---

## 10. Performance Characteristics

| Operation                               | Cost                                  |
|-----------------------------------------|---------------------------------------|
| Primitive function call (host → script) | Near-zero — stack push + jump         |
| Primitive type access                   | Zero copy — direct memory access      |
| Collection allocation                   | Reference-counted heap allocation     |
| Instruction limit check                 | One counter decrement per instruction |
| Coroutine switch                        | Stack swap — no heap allocation       |

---

## 11. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
