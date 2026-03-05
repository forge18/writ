# Writ Architecture

## 1. System Overview

Writ is a statically typed, embedded scripting language designed for games and applications. It provides GDScript's approachability with near-zero Rust interop cost. Scripts are compiled to bytecode and executed on a lightweight VM embedded in the Rust host.

**File extension:** `.writ`

**Primary use case:** Embedded scripting inside games and applications with a Rust host.

**Design philosophy:**

- GDScript's approachability and game-dev ergonomics
- C#-inspired syntax, cleaned up and modernized
- Low cognitive complexity — easy to learn, scales to powerful usage
- Clean Rust interop — near-zero marshalling cost
- Small surface area — no legacy baggage

### Crate Structure

| Crate | Description |
|---|---|
| `writ-lexer` | Tokenizes `.writ` source files into a token stream |
| `writ-parser` | Converts token stream into an AST |
| `writ-types` | Type checker — validates types, resolves names, checks trait implementations |
| `writ-compiler` | Bytecode compiler — converts type-checked AST to bytecode |
| `writ-vm` | Bytecode VM — executes compiled scripts, manages coroutines, enforces instruction limits |
| `writ-stdlib` | Standard library — math, string, array, dictionary, I/O, time, random |
| `writ-lsp` | Language server — exposes compiler pipeline results to editors via LSP |

---

## 2. Compilation Pipeline

```
Source → Lexer → Parser → AST → Type Checker → Bytecode Compiler → Bytecode → VM
```

Each stage is a discrete, testable component. The LSP reuses the lexer, parser, and type checker to provide autocomplete, errors, and hover information.

---

## 3. Guiding Principles

**Static typing required.** All variables, parameters, and return types are statically typed. Type inference reduces annotation burden but does not introduce dynamic typing. All type errors are caught before a single instruction executes.

**Near-zero Rust interop cost.** Script types map directly to Rust types in memory. Primitives have identical memory layout to their Rust equivalents. User-defined types compile to first-class Rust types, not VM-managed wrappers. The host registers functions and types explicitly — the VM starts with zero external access.

**Capability-based sandboxing.** The VM exposes nothing by default. The host opts in by registering types, functions, and callbacks. Different VM instances can have different API surfaces — mod scripts get a restricted surface, core game scripts get full access.

**Lua's embeddability philosophy.** Like Lua, Writ is a small, self-contained VM that any Rust application can embed. The standard library covers universal utilities. Domain-specific functionality is registered by the host. The language is reusable across different hosts.

---

## 4. System Map

```
┌─────────────────────────────────────────────────────┐
│                      writ-vm                        │
│                                                     │
│  ┌───────────────┐  ┌───────────────┐               │
│  │  Bytecode VM  │  │  Coroutine    │               │
│  │  - stack      │  │  Scheduler   │               │
│  │  - heap       │  │  - structured │               │
│  │  - call stack │  │    concurrency│               │
│  └───────────────┘  └───────────────┘               │
│                                                     │
│  ┌───────────────┐  ┌───────────────┐               │
│  │  Debug        │  │  Instruction  │               │
│  │  - breakpoints│  │  Limit        │               │
│  │  - hooks      │  │  - mod safety │               │
│  │  - hot reload │  └───────────────┘               │
│  └───────────────┘                                  │
│                                                     │
│  ┌───────────────┐                                  │
│  │  Binding Layer│                                  │
│  │  - register_  │                                  │
│  │    type/fn    │                                  │
│  │  - sandbox    │                                  │
│  └───────────────┘                                  │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│              writ-compiler                          │
│  - AST → bytecode                                   │
│  - type-verified, no runtime type checks needed     │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│               writ-types                            │
│  - type inference                                   │
│  - name resolution                                  │
│  - trait impl validation                            │
│  - null safety                                      │
│  - result handling                                  │
│  - Rust-style error messages                        │
└─────────────────────────────────────────────────────┘

┌──────────────────┐  ┌──────────────────────────────┐
│   writ-parser    │  │         writ-lexer            │
│  - AST           │  │  - token stream               │
│  - grammar rules │  │  - keyword recognition        │
└──────────────────┘  └──────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│              writ-stdlib                            │
│  Basic | Math | String | Array | Dictionary         │
│  I/O | Time | Random                               │
│  Backed by Rust std where possible                  │
└─────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────┐
│               writ-lsp                              │
│  - autocomplete (from type checker)                 │
│  - go to definition                                 │
│  - find references                                  │
│  - inline errors                                    │
│  - hover documentation                              │
│  - rename refactoring                               │
└─────────────────────────────────────────────────────┘
```

---

## 5. System Specifications

### Language (`language/`)

| System | Spec File | Scope |
|---|---|---|
| Type System | [language/type-system.md](language/type-system.md) | Primitives, Optional, Result, generics, traits, enums, type inference, null safety |
| Syntax | [language/syntax.md](language/syntax.md) | Variables, functions, classes, control flow, lambdas, coroutines, modules |
| Standard Library | [language/stdlib.md](language/stdlib.md) | Basic, math, string, array, dictionary, I/O, time, random |

### Runtime (`runtime/`)

| System | Spec File | Scope |
|---|---|---|
| VM | [runtime/vm.md](runtime/vm.md) | Bytecode execution, stack, heap, call stack, memory management |
| Rust Interop | [runtime/rust-interop.md](runtime/rust-interop.md) | Type mapping, inheritance compilation, binding model, sandboxing |
| Coroutines | [runtime/coroutines.md](runtime/coroutines.md) | Structured concurrency, yield variants, lifetime management |
| Debug | [runtime/debug.md](runtime/debug.md) | Stack traces, breakpoints, debug hooks, hot reload, instruction limits |

### Tooling (`tooling/`)

| System | Spec File | Scope |
|---|---|---|
| LSP | [tooling/lsp.md](tooling/lsp.md) | Language server protocol implementation |
| VS Code Extension | [tooling/vscode.md](tooling/vscode.md) | Syntax highlighting, debugger integration, hot reload |
| Error Messages | [tooling/error-messages.md](tooling/error-messages.md) | Error format, suggestions, context display |

---

## 6. Design Decisions

See [DECISIONS.md](DECISIONS.md) for the complete decision log.
