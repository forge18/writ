# Design Decisions

Centralized log of architectural and design decisions for Writ. Each entry records what was decided, why, and when.

## Language Decisions

| Date | Decision | Rationale |
|---|---|---|
| 2026-03-02 | Static typing required, not optional | Catch errors before runtime, enable zero-cost Rust interop, eliminate runtime type checks |
| 2026-03-02 | Primitives map directly to Rust equivalents | int→i32, float→f32, etc. Identical memory layout means zero conversion cost at the FFI boundary |
| 2026-03-02 | Private visibility by default | Consistent with Rust. `public` to expose, `private` optional for explicitness |
| 2026-03-02 | `const`/`let`/`var` for variable declaration | `const` = compile-time, `let` = runtime immutable, `var` = mutable. Clarity over brevity |
| 2026-03-02 | `Optional<T>` for nullability, not nullable types | Non-nullable by default. Explicit optionality with `.hasValue`, `??`, `?.`. Prevents null reference errors |
| 2026-03-02 | `Result<T>` with `Success`/`Error` for error handling | Explicit error handling. `?` propagation. No exceptions. Rare in scripting languages but justified by static typing |
| 2026-03-02 | Single inheritance via `extends`, multiple traits via `with` | Scala-style. One concept (trait) replaces interface/abstract/virtual. Maps to Rust traits. Default implementations supported |
| 2026-03-02 | No events/signals as a language feature | Host calls script functions directly. The host already has an event loop. Events belong to the engine, not the language |
| 2026-03-02 | Kotlin-style `when` for pattern matching | Value matching, type matching with binding, guard clauses, range matching. Exhaustive with `else` requirement |
| 2026-03-02 | Coroutines over async/await | Game scripting needs pause-and-resume across frames, not I/O non-blocking. Coroutines solve the real problem |
| 2026-03-02 | GDScript-style coroutine syntax, Kotlin-style structured concurrency | GDScript's `yield` syntax is well-loved. Kotlin's structured concurrency fixes GDScript's dangling coroutine problem |
| 2026-03-02 | TypeScript-style module system, named exports only | Explicit, traceable dependencies. No default exports to avoid ambiguity. `import { X } from "path"` |
| 2026-03-02 | Host/engine types always globally available, never imported | Matches Lua, Rhai, GDScript. Only script-to-script references need imports |
| 2026-03-02 | `::` for namespace access on wildcard imports | Rust-familiar. Distinguishes namespace access (`weapon::Weapon`) from field access (`weapon.attack()`) |
| 2026-03-02 | Java-style enums with fields and methods | More expressive than simple enums. Useful for game state, direction, status values |
| 2026-03-02 | Auto-generated default and memberwise constructors | Kotlin-style. Custom constructor only when special logic needed |
| 2026-03-02 | Kotlin-style property syntax for getters/setters | Declare field normally, add accessor logic only when needed. No boilerplate for simple properties |
| 2026-03-02 | Go-style doc comments | Plain `//` above declarations. No special syntax. Tooling picks them up automatically |
| 2026-03-02 | Semicolons optional | Newlines terminate statements. Unclosed brackets defer termination |
| 2026-03-02 | String interpolation with `$name` and `${expression}` | GDScript-familiar. `\$` escapes interpolation. `"""` for multiline |
| 2026-03-02 | `..` for string concatenation | Avoids overloading `+`. Explicit operator for string-specific operation |
| 2026-03-02 | No operator overloading in the language | Rust binding layer handles it. Not exposed to scripters |

## Runtime Decisions

| Date | Decision | Rationale |
|---|---|---|
| 2026-03-02 | Bytecode VM, not tree-walk interpreter | Fast enough for per-frame game scripting. Consistent performance. Hot-reload possible. Scripts loaded at runtime |
| 2026-03-02 | Scripts loaded at runtime, not compiled ahead-of-time | Enables modding. Enables hot-reload. Consistent with scripting language expectations |
| 2026-03-02 | User-defined types compile to first-class Rust types | Not VM-managed wrappers. Identical memory layout. Near-zero marshalling at the boundary |
| 2026-03-02 | Single inheritance compiles to composition with `Deref` | Idiomatic Rust. Borrow-checker friendly. No macros needed. Child contains `base` field. `Deref` makes `player.name` work naturally |
| 2026-03-02 | Three-tier memory management: stack, reference counting, host ownership | Stack for locals, reference counting for heap objects, Rust owns entity-bound objects. No GC, no GC pauses |
| 2026-03-02 | Capability-based sandboxing via binding registration | VM starts with zero external access. Host explicitly registers types, functions, and callbacks. Different VM instances can have different API surfaces |
| 2026-03-02 | Stack traces with file, line, and function name | Non-negotiable for debugging. Every runtime error includes a full trace |
| 2026-03-02 | Hot reload built into the VM | Critical for game dev iteration speed. GDScript proved this. State preserved where possible |
| 2026-03-02 | Breakpoints via host API | Host sets breakpoints, VM pauses, host inspects state. Required for a usable debugger |
| 2026-03-02 | Debug hooks for line, call, return | Lua-style. Enables host to build custom tooling on top — profilers, loggers, coverage |
| 2026-03-02 | Instruction limit for mod safety | Prevents infinite loops in untrusted mod scripts from hanging the host. Configurable per VM instance |

## Standard Library Decisions

| Date | Decision | Rationale |
|---|---|---|
| 2026-03-02 | Lua's standard library philosophy | Small, complete, universal. Host provides domain-specific functionality. Language is reusable across different hosts |
| 2026-03-02 | Rust std where possible, external crates only when std can't cover it | Battle-tested, performant, maintained. Near-zero cost since script types map to Rust types |
| 2026-03-02 | `rand` crate for random numbers | Only external dependency. Rust std has no RNG |

## Tooling Decisions

| Date | Decision | Rationale |
|---|---|---|
| 2026-03-02 | LSP server ships with the language | Makes editor integration possible. Reuses the compiler pipeline — type checker already knows everything the LSP needs |
| 2026-03-02 | VS Code extension as primary editor target | Most common editor. LSP means other editors follow without separate work |
| 2026-03-02 | Rust-style error messages | Precise, plain English, actionable. File, line, column. Suggestions where possible. Elm and Rust set the bar |
| 2026-03-02 | No package manager, CLI, doc generator, formatter, or linter in initial scope | Scope management. Language first. Tooling follows when the language is stable |

## Naming

| Date | Decision | Rationale |
|---|---|---|
| 2026-03-02 | Language named Writ | A writ is a formal written command. Scripts are written commands to the engine. Short, real word, not taken as a programming language |
| 2026-03-02 | File extension `.writ` | Three letters, clearly derived from Writ, not associated with anything significant |
