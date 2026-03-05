# Debug

> **Crate:** `writ-vm` | **Status:** Draft

## 1. Purpose

This spec defines Writ's debug and developer tooling features — stack traces, hot reload, breakpoints, debug hooks, and instruction limits.

## 2. Dependencies

| Depends On                  | Relationship                                             |
|-----------------------------|----------------------------------------------------------|
| [vm.md](vm.md)              | All debug features are VM features                       |
| [lsp.md](../tooling/lsp.md) | Breakpoints triggered from the VS Code extension via LSP |

---

## 3. Stack Traces

All runtime errors include a full stack trace with file name, line number, and function name. Line number information is encoded into the bytecode at compile time — no runtime overhead when no error occurs.

**Format:**

```text
Error: Division by zero
  at divide (math/utils.writ:12)
  at calculateDamage (combat/damage.writ:34)
  at Player.takeDamage (entities/player.writ:67)
```

**Rules:**

- Every frame in the call stack is listed, innermost first
- Anonymous lambdas show as `<lambda>` with their definition site
- Coroutine frames show coroutine state (suspended/running)
- Host function frames show the Rust function name

Stack traces are returned to the host as structured data, not just a formatted string, so the host can display them in its own format.

```rust
pub struct StackTrace {
    pub frames: Vec<StackFrame>,
}

pub struct StackFrame {
    pub function: String,
    pub file: String,
    pub line: u32,
    pub is_native: bool,
}
```

---

## 4. Hot Reload

Scripts can be reloaded at runtime without restarting the host. The VM recompiles the changed file and swaps the bytecode in place. Running coroutines and existing object state are preserved where possible.

```rust
vm.reload("entities/player.writ")?;
```

**Behavior:**

- The new bytecode replaces the old bytecode for the reloaded module
- Existing object instances retain their current field values
- New fields added to a class since the last load are initialized to their declared defaults
- Removed fields are dropped from existing instances
- Function bodies are updated immediately — the next call uses the new implementation

**Failure handling:** If compilation fails (syntax error, type error), the existing bytecode remains active. The reload error is returned to the host.

---

## 5. Breakpoints

The host sets breakpoints on specific file/line combinations. When the VM reaches a breakpointed line, it pauses execution and calls the registered breakpoint handler.

```rust
vm.set_breakpoint("entities/player.writ", 67);

vm.on_breakpoint(|ctx: &BreakpointContext| {
    // Inspect state, step, continue, or abort
    println!("Paused at {}:{}", ctx.file, ctx.line);
    BreakpointAction::Continue
});
```

**BreakpointContext:**

```rust
pub struct BreakpointContext<'a> {
    pub file: &'a str,
    pub line: u32,
    pub function: &'a str,
    pub stack_trace: &'a StackTrace,
}
```

**BreakpointAction:**

```rust
pub enum BreakpointAction {
    Continue,     // Resume execution
    StepOver,     // Execute next line, pause again
    StepInto,     // Step into the next function call
    Abort,        // Terminate the script with an error
}
```

---

## 6. Debug Hooks

The host can register callbacks that fire on every line executed, every function call, and every function return. Enables custom tooling — profilers, loggers, coverage trackers — on top of the VM.

```rust
vm.on_line(|file: &str, line: u32| {
    // Called before every line is executed
});

vm.on_call(|fn_name: &str, file: &str, line: u32| {
    // Called when any function is entered
});

vm.on_return(|fn_name: &str, file: &str, line: u32| {
    // Called when any function returns
});
```

**Performance note:** Debug hooks have a non-trivial per-instruction cost. They are intended for development tooling only. The host should not register hooks in production builds.

---

## 7. Instruction Limit

The host sets a maximum instruction count per script execution. When the limit is hit, the script is terminated and an error is returned to the host. Protects against infinite loops in untrusted mod scripts.

```rust
vm.instruction_limit(1_000_000);  // terminate after 1M instructions
```

The counter resets on each top-level `vm.call()` invocation. Coroutines share the counter with their parent call.

**Error on limit exceeded:**

```text
Error: Instruction limit exceeded (1000000 instructions)
  at processLoop (scripts/ai.writ:45)
  at Enemy.update (entities/enemy.writ:12)
```

Setting `instruction_limit(0)` disables the limit entirely. Default: no limit.

---

## 8. Edge Cases

1. **Given** a breakpoint is set on a line that is never executed, **then** the handler is never called — no error.
2. **Given** a breakpoint handler returns `StepOver` inside a coroutine, **then** execution steps over the next line within that coroutine's frame.
3. **Given** hot reload is called while a coroutine is suspended mid-execution, **then** the coroutine resumes with the new bytecode on its next tick. If the reload changed the function the coroutine was inside, behavior is best-effort — the VM makes no guarantees about coroutine state across incompatible reloads.
4. **Given** the instruction limit is hit inside a deeply nested call, **then** the full stack trace up to the limit point is included in the error.
5. **Given** debug hooks are registered and a coroutine switches, **then** `on_call` and `on_return` fire for the resume and suspend respectively.

---

## 9. Performance Characteristics

| Feature           | Cost                                                                |
|-------------------|---------------------------------------------------------------------|
| Stack traces      | Zero at runtime; line numbers encoded in bytecode at compile time   |
| Breakpoints       | One hash lookup per line executed when breakpoints are active       |
| Debug hooks       | One callback invocation per line/call/return — non-trivial overhead |
| Instruction limit | One counter decrement per instruction — negligible                  |
| Hot reload        | Recompile changed file + bytecode swap — only on explicit call      |

---

## 10. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
