---
title: Debugging
description: Breakpoints, debug hooks, stack traces, error messages, and hot reload.
---

## Error messages

Writ produces Rust-style error messages — precise, plain English, actionable. The compiler always reports file, line, and column. Where possible it suggests a fix.

### Type mismatch

```
Error: Type mismatch
  --> entities/player.writ:34:12
   |
34 |     health = "full"
   |              ^^^^^^ expected float, found string
```

### Unknown field with suggestion

```
Error: Unknown field 'hp'
  --> entities/player.writ:34:12
   |
32 |     public func takeDamage(amount: float) {
33 |         self.hp -= amount
   |              ^^ no field named 'hp' on Player
34 |     }
   |
   = did you mean 'health'?
```

**Rules:**
- Always include file, line, and column
- Plain English — say what was expected and what was found
- Show surrounding lines for context
- Suggest fixes where the type checker can infer them
- Avoid cascading errors from a single mistake

---

## Stack traces

All runtime errors include a full stack trace with file name, line number, and function name:

```
Error: Division by zero
  at divide (math/utils.writ:12)
  at calculateDamage (combat/damage.writ:34)
  at Player.takeDamage (entities/player.writ:67)
```

---

## Breakpoints

The host can set breakpoints on specific lines. Execution pauses when a breakpoint is hit, allowing inspection of the current state.

```rust
vm.set_breakpoint("entities/player.writ", 67);
vm.on_breakpoint(|ctx| {
    println!("Hit breakpoint at {}:{}", ctx.file, ctx.line);
    writ::BreakpointAction::Continue
});
```

---

## Debug hooks

Enable via the `debug-hooks` feature in `Cargo.toml`:

```toml
writ = { version = "0.1", features = ["debug-hooks"] }
```

Register callbacks that fire on every line, call, or return. Enables the host to build custom tooling — debuggers, profilers, loggers — on top of the VM.

```rust
vm.on_line(|file, line| { /* called before each new source line */ });
vm.on_call(|file, fn_name, line| { /* called on function entry */ });
vm.on_return(|file, fn_name, line| { /* called on function return */ });
```

---

## Hot reload

Scripts can be reloaded at runtime without restarting the host. Only function bytecode is swapped — globals, live objects, and running coroutines are preserved. Scripts are fully re-type-checked on reload.

```rust
vm.reload("entities/player.writ").unwrap();
```

Call this in response to a file watcher event. Designed for fast iteration during development.

---

## Tooling

### Language Server Protocol (LSP)

Writ ships with an LSP server enabling editor integration. The LSP sits on top of the compiler pipeline — feeding source through the lexer, parser, and type checker and exposing results to the editor.

**Features:**
- Autocomplete
- Go to definition
- Find references
- Inline errors while typing
- Hover documentation
- Rename refactoring

### VS Code Extension

A VS Code extension ships alongside the LSP providing:
- Syntax highlighting
- Debugger integration via the VM's breakpoint API
- Hot reload trigger from the editor
- Error display inline
