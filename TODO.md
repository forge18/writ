# TODO

Architecture review findings from 2026-03-07. Ordered by priority.

---

## Critical

- [x] **Add a typed IR between `writ-types` and `writ-compiler`**
  The checker and compiler both walk the AST independently with separate type inference. A thin `TypedExpr { expr: Expr, ty: Type }` that the checker annotates and the compiler consumes would eliminate drift between the two and make typed instruction selection trivially correct. Do this before the AST grows further — retrofitting gets harder with every new language feature.
  _Files: `crates/writ-types/src/checker.rs`, `crates/writ-compiler/src/compiler.rs`_

- [x] **Fix `reload()` to go through the type checker**
  `Writ::reload()` calls `vm.reload()` directly, bypassing the lexer/parser/type checker. Hot-reloaded code is never type-checked. Fix: run the full pipeline in `reload()` the same way `load()` does, then hand the result to the VM.
  _File: `src/lib.rs:256-264`_

- [x] **Verify `Chunk` ownership at `load_module` time**
  Quickening mutates `Chunk::code` in place. If the same `Chunk` is shared across multiple VM instances, one VM's quickening corrupts bytecode for others. Confirm `execute_program` / `load_module` take ownership or deep-clone before the VM mutates instructions.
  _Files: `crates/writ-vm/src/vm.rs`, `crates/writ-compiler/src/chunk.rs`_

---

## Language Features

- [ ] **`super()` for parent method calls**
  Classes that override a parent method have no way to call the parent's implementation. `super.methodName(args)` (or equivalent) is needed for any non-trivial inheritance use case.
  _Files: `.spec/language/syntax.md`, `crates/writ-parser/src/ast.rs`, `crates/writ-compiler/src/compiler.rs`, `crates/writ-vm/src/vm.rs`_

- [ ] **File-wide type visibility (forward declarations)**
  The type checker requires types to be defined before they are referenced. Types that refer to each other via method signatures (e.g., `Enemy` has `func target() -> Player`, `Player` has `func getNearestEnemy() -> Enemy`) fail unless ordered carefully. Fix: extend the existing two-pass type registration to cover all type declarations before checking any bodies.
  _File: `crates/writ-types/src/checker.rs`_

- [ ] **Generic constraints (`where T : Trait`)**
  Generic functions can be declared but cannot constrain `T` to a trait, so calling any method on `T` inside the function is a type error. Add `where T : TraitName` syntax. Keep the implementation minimal — monomorphization handles the rest.
  _Files: `.spec/language/type-system.md`, `crates/writ-parser/src/ast.rs`, `crates/writ-types/src/checker.rs`_

- [ ] **Regex in stdlib**
  String stdlib covers searching and splitting but has no pattern extraction. Add a `Regex` type (backed by the `regex` crate) with at minimum `.match()`, `.matchAll()`, and `.replace()`.
  _File: `crates/writ-stdlib/`_

- [ ] **Multi-return destructuring at call site**
  Functions returning tuples require an intermediate binding before destructuring (`let t = f(); let (x, y) = t`). Allow `let (x, y) = f()` directly. Parser and type checker already support tuple destructuring — this is a small extension to allow it on call expressions without a named intermediate.
  _Files: `crates/writ-parser/src/ast.rs`, `crates/writ-types/src/checker.rs`, `crates/writ-compiler/src/compiler.rs`_

---

## Moderate

- [x] **Merge host function registration into a single `Writ` method**
  Added `Writ::register_host_fn(name, params, return_type, handler)` that registers in both the VM and type checker atomically.
  _File: `src/lib.rs`_

- [x] **Stop exposing `vm_mut()` / `type_checker_mut()` as primary API**
  Added dedicated `Writ` methods for all debug hook capabilities (`set_breakpoint`, `remove_breakpoint`, `on_breakpoint`, `on_line`, `on_call`, `on_return`, all gated on `feature = "debug-hooks"`). Both escape hatches marked `#[doc(hidden)]`.
  _File: `src/lib.rs`_

- [x] **Add bounds check on `next_reg` in the compiler**
  Already implemented at `crates/writ-compiler/src/compiler.rs:241` — checks `reg == u8::MAX` and returns `CompileError` with message "too many local variables/temporaries (max 255)".
  _File: `crates/writ-compiler/src/compiler.rs:241`_

- [x] **Scope `TypeChecker` state per `run()` or add explicit module scoping**
  Added `Writ::reset_script_types()` as an opt-in escape hatch that replaces the type checker with a fresh instance. Also fixed `reload()` which was running `check_program()` then `check_program_typed()` on the same stmts (double type-check); now runs only `check_program_typed()`.
  _File: `src/lib.rs`_

---

## Low

- [x] **Provide default implementations for `WritObject` hash methods**
  Hash method defaults (`get_field_by_hash`, `set_field_by_hash`) already existed and delegate to the string variants. Added `'static` supertrait bound to `WritObject` to enforce that host types are `'static`, which is required for `Any` downcasting. Note: `as_any()` cannot have a default implementation — it requires vtable dispatch on `dyn WritObject`, and Rust's `where Self: Sized` guard would exclude it from the vtable.
  _File: `crates/writ-vm/src/object.rs`_

- [x] **Make `disable_type_checking()` more surgical**
  Added `Writ::register_host_fn_untyped(name, handler)` that registers with a single `Type::Unknown` param (sentinel). In `infer_call`, a guard before the arity check short-circuits to "accept any args, infer arg exprs for undefined-variable detection only". All other type checking continues normally.
  _Files: `src/lib.rs`, `crates/writ-types/src/checker.rs`_

- [x] **Fix `codegen_rust()` to use existing `TypeChecker` state**
  Changed `check_program` to `check_program_typed` — consistent with the pipeline used in `run()`/`load()`. The shared `self.type_checker` accumulates state from all prior `load()` calls; the typed stmts return is discarded since codegen reads directly from `registry()`.
  _File: `src/lib.rs`_

- [x] **Investigate `Rc<RefCell<...>>` overhead for arrays/dicts in tight loops**
  Investigated 2026-03-07. `Rc<RefCell<...>>` overhead is O(1) per access and unavoidable — shared-reference semantics require it. No default-on change to `Value` warranted. Fixed concrete bug found: AoSoA `map`/`filter`/`for_each` called `container.borrow()` on every loop iteration (N borrow-checks for N elements). Fixed by snapshotting elements into a `Vec` before the loop, releasing the borrow before `call_function` runs.
  _File: `crates/writ-vm/src/vm.rs`_
