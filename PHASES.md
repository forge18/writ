# Writ Implementation Phases

## Context

Writ is a spec-driven embedded scripting language in Rust. All architecture and system behavior is specified in `.spec/`. Zero implementation code has been written. The goal is to implement in small, verifiable chunks starting from the most foundational layer and building upward — each phase runnable and tested before moving to the next.

**Guiding constraints:**

- Each phase must be runnable/testable before moving to the next
- Phases are deliberately small — a few hundred lines of code at most before testing
- Write tests as you write code, not after
- Build order respects dependency direction: lexer → parser → type checker → compiler → VM
- Every error path is tested, not just the happy path
- No phase depends on a later phase — each phase is complete when its tests pass

---

## Phase 1 — Workspace Scaffold + `writ-lexer` - Completed

**Why first:** The lexer is the entry point to the entire pipeline. Everything depends on it. It has no internal dependencies. It is the most mechanical part to write — read a string, emit tokens. Easy to test exhaustively with unit tests.

**Deliverables:**

- `Cargo.toml` workspace listing all crates
- `crates/writ-lexer/Cargo.toml`
- `Token` enum covering all tokens in the language:
  - Keywords: `class`, `trait`, `enum`, `func`, `let`, `var`, `const`, `public`, `private`, `static`, `extends`, `with`, `import`, `export`, `return`, `if`, `else`, `when`, `while`, `for`, `in`, `break`, `continue`, `is`, `as`, `self`, `start`, `yield`, `true`, `false`
  - Literals: integer, float, string (including interpolated segments), bool
  - Identifiers
  - Operators: `+`, `-`, `*`, `/`, `%`, `=`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`, `?`, `??`, `?.`, `..`, `...`, `->`, `=>`, `+=`, `-=`, `*=`, `/=`, `%=`
  - Delimiters: `(`, `)`, `{`, `}`, `[`, `]`, `,`, `:`, `;`
  - Range operators: `..`, `..=`
  - Comments: single-line `//`, multi-line `/* */`
  - Whitespace and newlines (for optional semicolons)
  - EOF
- `Lexer` struct
  - `new(source: &str) -> Lexer`
  - `next_token() -> Token` — advances one token
  - `tokenize() -> Vec<Token>` — collect all tokens
- `Span` struct: `{ file: String, line: u32, column: u32, length: u32 }` — every token carries a span
- `LexError` type with file + line + column + message
- String interpolation: tokenize `"Hello $name"` into `StringStart`, `StringLiteral("Hello ")`, `Interpolation`, `Identifier("name")`, `StringEnd` segments

**Tests (`crates/writ-lexer/tests/`):**

- `test_keywords` — every keyword tokenizes correctly
- `test_operators` — every operator tokenizes correctly
- `test_integer_literals` — `0`, `100`, `-5`, max i32
- `test_float_literals` — `0.0`, `3.14`, `-1.5`
- `test_string_literal` — plain string, string with escapes
- `test_string_interpolation` — `"Hello $name"`, `"${a + b}"`
- `test_multiline_string` — `""" ... """`
- `test_comments_skipped` — `// comment` and `/* comment */` produce no tokens
- `test_span_tracking` — tokens carry correct line and column
- `test_newline_terminates_statement` — newline produces `Newline` token
- `test_empty_source` — produces only `EOF`
- `test_unknown_character` — produces `LexError`

**Verification:** `cargo test -p writ-lexer` — all tests pass.

**Spec refs:** `.spec/language/syntax.md`

---

## Phase 2 — `writ-parser` (Expressions + Literals) - Completed

**Why this chunk:** Parse only expressions and literals first — the smallest meaningful subset of the grammar. No statements, no declarations, no functions, no classes yet. Validates the AST design before committing to it across the whole language.

**Deliverables:**

- `crates/writ-parser/Cargo.toml`
- AST node types for expressions:
  - `Literal`: integer, float, string, bool
  - `Identifier(String)`
  - `BinaryExpr { op, lhs, rhs }`
  - `UnaryExpr { op, operand }`
  - `Grouped(Box<Expr>)` — parenthesized expression
  - `TernaryExpr { condition, then_expr, else_expr }`
  - `RangeExpr { start, end, inclusive }`
  - `NullCoalesce { lhs, rhs }`
  - `SafeAccess { object, member }`
  - `StringInterpolation(Vec<InterpolationSegment>)` — literal segments + expression segments
- `Parser` struct
  - `new(tokens: Vec<Token>) -> Parser`
  - `parse_expr() -> Result<Expr, ParseError>`
- `ParseError` type with span + message

**Tests (`crates/writ-parser/tests/`):**

- `test_integer_literal`
- `test_float_literal`
- `test_string_literal`
- `test_string_interpolation`
- `test_binary_add`
- `test_binary_precedence` — `1 + 2 * 3` parses as `1 + (2 * 3)`
- `test_unary_negate`
- `test_unary_not`
- `test_grouped`
- `test_ternary`
- `test_range_exclusive`
- `test_range_inclusive`
- `test_null_coalesce`
- `test_safe_access`
- `test_chained_access` — `a?.b?.c`
- `test_parse_error_missing_rhs` — `1 +` produces `ParseError`

**Verification:** `cargo test -p writ-parser` — all tests pass.

**Spec refs:** `.spec/language/syntax.md`

---

## Phase 3 — `writ-parser` (Statements + Control Flow) - Completed

**Why this chunk:** Statements and control flow on top of the expression layer. Still no declarations (no functions, classes, traits yet). Validates that the statement grammar composes correctly with expressions.

**Deliverables:**

- AST node types for statements:
  - `LetDecl { name, type_annotation?, initializer }`
  - `VarDecl { name, type_annotation?, initializer }`
  - `ConstDecl { name, initializer }`
  - `Assignment { target, op, value }` — `=`, `+=`, `-=`, etc.
  - `ExprStatement(Expr)` — expression used as a statement
  - `Return(Option<Expr>)`
  - `Break`, `Continue`
  - `Block(Vec<Stmt>)`
  - `IfStmt { condition, then_block, else_branch }` — `else_branch` is `Option<IfStmt | Block>`
  - `WhileStmt { condition, body }`
  - `ForStmt { variable, iterable, body }`
  - `WhenStmt { subject?, arms }` — subject is `Option<Expr>`
  - `WhenArm { pattern, body }`
  - `WhenPattern` variants: value, multiple values, range, type match `is T(binding)`, guard
- `parse_stmt() -> Result<Stmt, ParseError>`
- `parse_block() -> Result<Block, ParseError>`

**Tests (`crates/writ-parser/tests/`):**

- `test_let_decl_with_type`
- `test_let_decl_inferred`
- `test_var_decl`
- `test_const_decl`
- `test_assignment`
- `test_compound_assignment` — `+=`, `-=`, etc.
- `test_if_else`
- `test_if_else_if_else`
- `test_while`
- `test_for_in_array`
- `test_for_in_range`
- `test_when_value_matching`
- `test_when_multiple_values`
- `test_when_range`
- `test_when_type_match`
- `test_when_guard`
- `test_when_no_subject`
- `test_when_multiline_arm`
- `test_return_value`
- `test_return_void`
- `test_break`, `test_continue`
- `test_optional_semicolons` — statements with and without semicolons parse the same

**Verification:** `cargo test -p writ-parser` — all tests pass.

**Spec refs:** `.spec/language/syntax.md`

---

## Phase 4 — `writ-parser` (Declarations) - Completed

**Why this chunk:** The full grammar — functions, classes, traits, enums, and modules. After this phase the parser is complete.

**Deliverables:**

- AST node types for declarations:
  - `FuncDecl { name, params, return_type?, body, is_static }`
  - `LambdaExpr { params, body }` — expression-level
  - `ClassDecl { name, extends?, traits, fields, methods }`
  - `FieldDecl { name, type_annotation, default?, visibility, setter? }`
  - `TraitDecl { name, methods }`
  - `TraitMethod { name, params, return_type?, default_body? }`
  - `EnumDecl { name, variants, fields, methods }`
  - `EnumVariant { name, value? }`
  - `ImportDecl { names, from }` — named imports
  - `WildcardImport { alias, from }`
  - `ExportDecl { declaration }` — wraps any top-level decl
  - `TupleDecl`, `TupleDestructure`
- `parse_decl() -> Result<Decl, ParseError>`
- `parse_file() -> Result<Vec<Decl>, ParseError>` — top-level entry point

**Tests (`crates/writ-parser/tests/`):**

- `test_func_decl`
- `test_func_with_return_type`
- `test_func_variadic`
- `test_lambda_single_expr`
- `test_lambda_block`
- `test_static_func`
- `test_class_empty`
- `test_class_extends`
- `test_class_with_traits`
- `test_class_field_visibility`
- `test_class_field_setter`
- `test_class_constructor_call` — `Player(name: "Hero")` in expression position
- `test_trait_decl`
- `test_trait_with_default`
- `test_enum_simple`
- `test_enum_with_values`
- `test_enum_with_methods`
- `test_named_import`
- `test_wildcard_import`
- `test_export_class`
- `test_export_func`
- `test_tuple_decl`
- `test_tuple_destructure`
- `test_full_file` — a realistic `.writ` file parses without error

**Verification:** `cargo test -p writ-parser` — all tests pass.

**Spec refs:** `.spec/language/syntax.md`

---

## Phase 5 — `writ-types` (Primitives + Variables) - Completed

**Why this chunk:** Start the type checker with the simplest case — primitive types and variable declarations. No functions, no classes, no inference of complex types yet. Validates the type environment design.

**Deliverables:**

- `crates/writ-types/Cargo.toml`
- `Type` enum: `Int`, `BigInt`, `UInt`, `UBigInt`, `Float`, `BigFloat`, `Bool`, `Str`, `Void`, `Unknown`
- `TypeEnv` struct — scoped symbol table
  - `new()`
  - `push_scope()` / `pop_scope()`
  - `define(name: &str, ty: Type)`
  - `lookup(name: &str) -> Option<&Type>`
- `TypeChecker` struct
  - `check_decl(decl: &Decl) -> Result<(), TypeError>`
  - `check_stmt(stmt: &Stmt) -> Result<(), TypeError>`
  - `infer_expr(expr: &Expr) -> Result<Type, TypeError>`
- `TypeError` type with span + message
- Type checking for:
  - `LetDecl`, `VarDecl`, `ConstDecl` — type annotation vs inferred type
  - Assignment — target type must match value type
  - Arithmetic expressions — operand type compatibility
  - Comparison expressions — result is `Bool`
  - Logical expressions — operands must be `Bool`

**Tests (`crates/writ-types/tests/`):**

- `test_let_int_literal`
- `test_let_float_literal`
- `test_let_string_literal`
- `test_let_bool_literal`
- `test_let_annotation_matches`
- `test_let_annotation_mismatch` — expects `TypeError`
- `test_var_reassign_same_type`
- `test_var_reassign_wrong_type` — expects `TypeError`
- `test_const_inferred`
- `test_binary_arithmetic_type`
- `test_binary_comparison_returns_bool`
- `test_logical_and_requires_bool` — `1 && true` expects `TypeError`
- `test_scope_isolation` — variable in inner scope not visible in outer
- `test_undefined_variable` — expects `TypeError`

**Verification:** `cargo test -p writ-types` — all tests pass.

**Spec refs:** `.spec/language/type-system.md`

---

## Phase 6 — `writ-types` (Functions + Return Types) - Completed

**Why this chunk:** Function declarations with parameter types and return type checking. Still no classes, traits, or generics. Tests error propagation with `?` and `Result<T>`.

**Deliverables:**

- `Type` extended with: `Result(Box<Type>)`, `Optional(Box<Type>)`, `Tuple(Vec<Type>)`
- Function type checking:
  - Parameter type validation at call sites
  - Return type inference and validation (all branches must return the declared type)
  - Variadic parameter type checking
  - `void` return type (and missing return type treated as `void`)
- `?` operator: only valid inside a function returning `Result<T>` — `TypeError` otherwise
- Lambda type checking: parameter types from annotation, return type inferred from body
- `Result<T>` and `Optional<T>` structural types
- `when` over `Result<T>`: exhaustiveness check (`is Success` + `is Error` required)
- `??` operator on `Optional<T>` and `Result<T>`

**Tests:**

- `test_func_correct_return`
- `test_func_wrong_return_type` — expects `TypeError`
- `test_func_missing_return_on_branch` — expects `TypeError`
- `test_func_void_no_return`
- `test_call_correct_args`
- `test_call_wrong_arg_count` — expects `TypeError`
- `test_call_wrong_arg_type` — expects `TypeError`
- `test_propagate_operator_in_result_func`
- `test_propagate_operator_outside_result_func` — expects `TypeError`
- `test_result_when_exhaustive`
- `test_result_when_missing_arm` — expects `TypeError`
- `test_optional_null_coalesce`
- `test_optional_safe_access`
- `test_optional_non_nullable_assignment` — `let x: string = null` expects `TypeError`
- `test_lambda_inferred_return`
- `test_lambda_wrong_param_count` — expects `TypeError`
- `test_tuple_destructure_types`

**Verification:** `cargo test -p writ-types` — all tests pass.

**Spec refs:** `.spec/language/type-system.md`

---

## Phase 7 — `writ-types` (Classes + Traits) - Completed

**Why this chunk:** Classes, inheritance, and trait implementation. The most complex part of the type checker.

**Deliverables:**

- `Type` extended with: `Class(String)`, `Trait(String)`, `Enum(String)`
- Class type checking:
  - Field declaration types
  - Method signatures and return types
  - Visibility enforcement (`private` fields not accessible from outside the class)
  - `self` type resolution inside methods
  - Inheritance: `extends` — child inherits parent fields and methods; child type is assignable where parent is expected
  - Trait implementation: all non-default methods must be implemented; default methods inherited
  - Multiple traits via `with`: conflicts detected (same method name in two traits without override) — `TypeError`
  - Auto-generated constructor type: `Player(name: string, health: float) -> Player`
  - Setter type: `set(value: T)` — value must match field type
- Enum type checking:
  - Variant access: `Direction.North` resolves to `Direction` type
  - Method signatures on enums
  - `when` over enum: exhaustiveness enforced (all variants or `else`)

**Tests:**

- `test_class_field_access`
- `test_class_field_wrong_type` — expects `TypeError`
- `test_class_private_field_external_access` — expects `TypeError`
- `test_class_method_self`
- `test_class_constructor_all_fields`
- `test_class_constructor_named_params`
- `test_class_extends_inherits_fields`
- `test_class_extends_assignable`
- `test_class_trait_impl_complete`
- `test_class_trait_impl_missing_method` — expects `TypeError`
- `test_class_trait_default_method_inherited`
- `test_class_trait_method_override`
- `test_class_two_traits_conflict` — expects `TypeError`
- `test_class_setter_type`
- `test_class_setter_wrong_type` — expects `TypeError`
- `test_enum_variant_access`
- `test_enum_method_call`
- `test_when_enum_exhaustive`
- `test_when_enum_missing_variant` — expects `TypeError`
- `test_when_enum_with_else`

**Verification:** `cargo test -p writ-types` — all tests pass.

**Spec refs:** `.spec/language/type-system.md`

---

## Phase 8 — `writ-types` (Modules + Generics) - Completed

**Why this chunk:** Module resolution and generic collection types. After this phase the type checker is complete.

**Deliverables:**

- Module resolution:
  - `import { Weapon } from "items/weapon"` — resolves to the exported type from the target file
  - `import * as enemy from "entities/enemy"` — namespace import; `enemy::Enemy` resolves correctly
  - Export validation: only `export`-marked declarations are importable
  - Error on unknown import path, unknown exported name
- Generic collection types: `Array<T>`, `Dictionary<K, V>` fully typed
  - Method calls on typed arrays and dictionaries (`push`, `map`, `filter`, etc.) type-checked
  - Spread operator type checked: `[...arr1, ...arr2]` requires matching element types
- `as` cast type checking — allowed casts only
- Host-registered type integration: the type checker accepts a registry of host-provided type names and their exported methods; treats them as globally available

**Tests:**

- `test_named_import_resolves`
- `test_named_import_unknown_path` — expects `TypeError`
- `test_named_import_unknown_name` — expects `TypeError`
- `test_wildcard_import_namespace_access`
- `test_export_not_accessible_without_import` — expects `TypeError`
- `test_array_push_type`
- `test_array_map_returns_typed_array`
- `test_array_filter_predicate_type`
- `test_dictionary_contains_type`
- `test_spread_array_matching_types`
- `test_spread_array_mismatched_types` — expects `TypeError`
- `test_cast_int_to_float`
- `test_cast_invalid` — expects `TypeError`
- `test_host_registered_type_globally_available`
- `test_host_registered_type_method_call`

**Verification:** `cargo test -p writ-types` — all tests pass.

**Spec refs:** `.spec/language/type-system.md`

---

## Phase 9 — `writ-compiler` (Bytecode Design + Literals) - Completed

**Why this chunk:** Design the bytecode instruction set and emit it for the simplest case — literal values, arithmetic, and variable access. No control flow, no functions, no closures yet.

**Deliverables:**

- `crates/writ-compiler/Cargo.toml`
- `Instruction` enum — initial subset:
  - `LoadInt(i32)`, `LoadFloat(f32)`, `LoadBool(bool)`, `LoadStr(u32)` (string table index)
  - `LoadLocal(u8)` — load local variable by slot index
  - `StoreLocal(u8)` — store to local variable slot
  - `Add`, `Sub`, `Mul`, `Div`, `Mod`
  - `Neg` (unary negate)
  - `Not` (unary boolean not)
  - `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`
  - `And`, `Or`
  - `Return`
  - `Pop`
- `Chunk` struct: `Vec<Instruction>` + constant pool + line number table
- `Compiler` struct
  - `compile_expr(expr: &Expr) -> Result<(), CompileError>`
  - `compile_stmt(stmt: &Stmt) -> Result<(), CompileError>` (let/var/const/assignment only in this phase)
  - `emit(instruction: Instruction)`
  - `chunk() -> &Chunk`
- Local variable slot allocation
- `CompileError` type

**Tests (`crates/writ-compiler/tests/`):**

- `test_emit_int_literal` — `42` emits `LoadInt(42)`
- `test_emit_float_literal`
- `test_emit_bool_literal`
- `test_emit_string_literal` — string interned, `LoadStr(0)` emitted
- `test_emit_add`
- `test_emit_operator_precedence` — `1 + 2 * 3` emits in correct order
- `test_emit_unary_negate`
- `test_emit_comparison`
- `test_emit_let_decl` — variable assigned a slot
- `test_emit_var_load` — reading a variable emits `LoadLocal(slot)`
- `test_emit_assignment` — `x = 5` emits `LoadInt(5)`, `StoreLocal(slot)`
- `test_emit_compound_assignment` — `x += 1`

**Verification:** `cargo test -p writ-compiler` — all tests pass.

**Spec refs:** `.spec/runtime/vm.md`

---

## Phase 10 — `writ-compiler` (Control Flow + Functions) - Completed

**Why this chunk:** Jump instructions for control flow, and function call/return mechanics. After this phase the compiler handles most real-world scripts.

**Deliverables:**

- New instructions:
  - `Jump(i32)` — unconditional jump (relative offset)
  - `JumpIfFalse(i32)` — conditional jump
  - `JumpIfTrue(i32)` — for `||` short-circuit
  - `Call(u8)` — call function by constant pool index, N args
  - `CallNative(u32)` — call host-registered function by ID
  - `NullCoalesce` — pop top, use fallback if null
  - `Concat` — string concatenation
  - `MakeArray(u16)` — pop N items, make `Array<T>`
  - `MakeDict(u16)` — pop N key/value pairs, make `Dictionary<K,V>`
  - `GetField(u32)` — field access by name hash
  - `SetField(u32)` — field assignment
  - `GetIndex` — array index
  - `SetIndex` — array index assignment
  - `Spread` — spread array/dict into enclosing collection
- Compile `if`/`else` — emit `JumpIfFalse`, patch target offset after compiling body
- Compile `while` — emit loop-back jump + exit jump
- Compile `for` — emit range/iterator setup + loop
- Compile `when` — emit chain of comparisons + jumps
- Compile function declarations — each function gets its own `Chunk`; `FunctionTable` maps name → Chunk index
- Compile function calls — emit `Call` with arg count
- Compile `return`

**Tests:**

- `test_compile_if_true_branch`
- `test_compile_if_false_branch`
- `test_compile_if_else`
- `test_compile_while`
- `test_compile_for_range`
- `test_compile_for_in_array`
- `test_compile_when_value`
- `test_compile_when_else`
- `test_compile_function_decl`
- `test_compile_function_call`
- `test_compile_return_value`
- `test_compile_string_concat`
- `test_compile_array_literal`
- `test_compile_dict_literal`
- `test_compile_field_access`
- `test_compile_field_assign`

**Verification:** `cargo test -p writ-compiler` — all tests pass.

**Spec refs:** `.spec/runtime/vm.md`

---

## Phase 11 — `writ-vm` (Core Execution) - Completed

**Why this chunk:** The VM executes the bytecode the compiler emits. Start with literals, arithmetic, variables, and function calls. No host interop, no coroutines, no debug features yet.

**Deliverables:**

- `crates/writ-vm/Cargo.toml`
- `Value` enum: `Int(i32)`, `BigInt(i64)`, `UInt(u32)`, `UBigInt(u64)`, `Float(f32)`, `BigFloat(f64)`, `Bool(bool)`, `Str(Rc<String>)`, `Null`, `Array(Rc<RefCell<Vec<Value>>>)`, `Dict(Rc<RefCell<HashMap<Value, Value>>>)`
- `CallFrame` struct: chunk reference + program counter + base stack pointer
- `VM` struct
  - `new() -> VM`
  - `execute(chunk: &Chunk) -> Result<Value, RuntimeError>`
  - Operand stack
  - Call stack (`Vec<CallFrame>`)
  - Local variable slots (indexed off frame base)
- `RuntimeError` with message + stack trace (file + line per frame)
- Implement all instructions from Phase 9 and Phase 10

**Tests (`crates/writ-vm/tests/`):**

- `test_execute_int_literal`
- `test_execute_arithmetic`
- `test_execute_operator_precedence`
- `test_execute_boolean_logic`
- `test_execute_comparison`
- `test_execute_let_and_load`
- `test_execute_var_and_assign`
- `test_execute_if_true`
- `test_execute_if_false`
- `test_execute_while_loop`
- `test_execute_for_range`
- `test_execute_function_call`
- `test_execute_recursive_function`
- `test_execute_string_concat`
- `test_execute_array_literal`
- `test_execute_array_index`
- `test_execute_dict_literal`
- `test_execute_dict_access`
- `test_execute_return_from_function`
- `test_stack_trace_on_error`

**Verification:** `cargo test -p writ-vm` — all tests pass.

**Spec refs:** `.spec/runtime/vm.md`

---

## Phase 12 — `writ-vm` (Host Interop + Sandboxing) - Completed

**Why this chunk:** The host registers Rust types and functions into the VM. The VM enforces the sandbox — no access to unregistered capabilities.

**Deliverables:**

- `VM::register_type::<T>()` — exposes a Rust type to scripts
- `VM::register_fn(name, fn)` — exposes a Rust function to scripts
- `VM::disable_module(name)` — blocks a stdlib module
- `VM::instruction_limit(n)` — sets max instruction count
- `CallNative` instruction implementation — looks up registered function by ID, marshals args, calls Rust function, pushes return value
- `WritType` derive macro (or trait) — marks a Rust type as script-accessible
- `Value::Object(...)` variant for host-owned types
- Sandbox enforcement: script calls to unregistered functions produce `RuntimeError`
- Instruction counter: decrement on each instruction; error when limit reached

**Tests:**

- `test_register_fn_callable`
- `test_register_fn_wrong_arg_type` — expects `RuntimeError`
- `test_unregistered_fn_errors`
- `test_register_type_field_access`
- `test_register_type_method_call`
- `test_disable_module_blocks_calls`
- `test_instruction_limit_exceeded`
- `test_instruction_limit_reset_per_call`
- `test_two_vms_isolated_registrations`

**Verification:** `cargo test -p writ-vm` — all tests pass.

**Spec refs:** `.spec/runtime/rust-interop.md`, `.spec/runtime/vm.md §7`

---

## Phase 13 — `writ-vm` (Coroutines) - Completed

**Why this chunk:** Coroutines are the most complex VM feature. Implementing them after basic execution is solid.

**Deliverables:**

- `Coroutine` struct: own call stack + operand stack + program counter + state (Running / Suspended / Complete / Cancelled)
- `VM::tick(delta: f64)` — advances the coroutine scheduler one frame
- `start expr` compilation: emits `StartCoroutine` instruction
- `yield` compilation: emits `Yield` instruction (bare — one frame)
- `yield waitForSeconds(n)` — emits `YieldSeconds(f32)`
- `yield waitForFrames(n)` — emits `YieldFrames(u32)`
- `yield waitUntil(fn)` — emits `YieldUntil` + lambda reference
- `yield anotherCoroutine()` — emits `YieldCoroutine`
- Coroutine return values: `yield coroutine()` receives the return value when coroutine completes
- Structured concurrency: coroutine ownership tracked per script object; `VM::cancel_coroutines_for(object_id)` cancels all
- Cancellation propagation to child coroutines

**Tests:**

- `test_coroutine_runs_across_frames`
- `test_yield_one_frame`
- `test_yield_seconds`
- `test_yield_frames`
- `test_yield_until_condition`
- `test_yield_coroutine_waits_for_child`
- `test_coroutine_return_value`
- `test_coroutine_cancel_on_owner_destroy`
- `test_coroutine_cancel_propagates_to_child`
- `test_multiple_coroutines_run_concurrently`
- `test_start_does_not_block`

**Verification:** `cargo test -p writ-vm` — all tests pass.

**Spec refs:** `.spec/runtime/coroutines.md`

---

## Phase 14 — `writ-vm` (Debug Features) - Completed

**Why this chunk:** Stack traces, breakpoints, debug hooks, and hot reload. All build on the VM internals from Phase 11.

**Deliverables:**

- `StackTrace` + `StackFrame` structs — returned on all `RuntimeError`s
- `VM::set_breakpoint(file, line)` — register a breakpoint
- `VM::on_breakpoint(handler)` — register the pause callback
- `BreakpointContext` + `BreakpointAction` enums
- `VM::on_line(handler)`, `VM::on_call(handler)`, `VM::on_return(handler)` — debug hooks
- `VM::reload(path)` — recompile and hot-swap a script file's bytecode
- Stack trace encoding: line numbers embedded in `Chunk` as a parallel `Vec<(instruction_index, line)>`

**Tests:**

- `test_stack_trace_single_frame`
- `test_stack_trace_multiple_frames`
- `test_stack_trace_lambda`
- `test_breakpoint_fires`
- `test_breakpoint_continue`
- `test_breakpoint_not_set_does_not_fire`
- `test_debug_hook_on_line`
- `test_debug_hook_on_call`
- `test_debug_hook_on_return`
- `test_hot_reload_updates_function`
- `test_hot_reload_preserves_state`
- `test_hot_reload_compile_error_preserves_previous`

**Verification:** `cargo test -p writ-vm` — all tests pass.

**Spec refs:** `.spec/runtime/debug.md`

---

## Phase 15 — `writ-stdlib` - Completed

**Why this chunk:** The standard library is now implementable — the VM can execute host functions (Phase 12). Register all stdlib modules.

**Deliverables:**

- `crates/writ-stdlib/Cargo.toml` with `rand` dependency
- Each stdlib module implemented as a set of `register_fn` calls:
  - `basic`: `print`, `assert`, `type`
  - `math`: all functions and constants from spec
  - `string`: all string methods
  - `array`: all array methods
  - `dictionary`: all dictionary methods
  - `io`: `readFile`, `writeFile`, `readLine`, `fileExists`
  - `time`: `now`, `elapsed`
  - `random`: `random`, `randomInt`, `randomFloat`, `shuffle`
- `stdlib::register_all(vm: &mut VM)` — registers all modules
- `stdlib::register_except(vm: &mut VM, excluded: &[&str])` — registers all except named modules

**Tests:**

- `test_print_calls_host_output`
- `test_assert_passes`
- `test_assert_fails` — expects `RuntimeError`
- `test_type_returns_name`
- `test_math_abs`, `test_math_clamp`, `test_math_sqrt`, `test_math_sin`, `test_math_pi`
- `test_string_len`, `test_string_trim`, `test_string_split`, `test_string_contains`
- `test_array_push_pop`, `test_array_map`, `test_array_filter`
- `test_dictionary_keys`, `test_dictionary_merge`
- `test_io_write_read_roundtrip`
- `test_io_disabled_when_module_excluded`
- `test_random_in_range`
- `test_shuffle_changes_order`

**Verification:** `cargo test -p writ-stdlib` — all tests pass.

**Spec refs:** `.spec/language/stdlib.md`

---

## Phase 16 — End-to-End Integration - Completed

**Why this chunk:** Run a realistic `.writ` script through the full pipeline — lexer → parser → type checker → compiler → VM — and verify it produces correct results.

**Deliverables:**

- `writ` root crate (or `writ-core` convenience crate)
  - `Writ::new() -> Writ` — creates VM with stdlib registered
  - `Writ::register_type::<T>() -> &mut Self`
  - `Writ::register_fn(name, fn) -> &mut Self`
  - `Writ::run(source: &str) -> Result<Value, WritError>` — full pipeline
  - `Writ::load(path: &str) -> Result<(), WritError>` — compile and store a file
  - `Writ::call(fn_name: &str, args: &[Value]) -> Result<Value, WritError>`
  - `Writ::tick(delta: f64)` — advance coroutine scheduler
- `WritError` enum: `Lex(LexError)`, `Parse(ParseError)`, `Type(TypeError)`, `Compile(CompileError)`, `Runtime(RuntimeError)`

**Tests (`integration/`):**

- `test_hello_world` — `print("Hello, World!")` runs without error
- `test_fibonacci` — recursive function returns correct result
- `test_class_instantiation` — create class, call method, check result
- `test_trait_dispatch` — call method via trait reference
- `test_coroutine_integration` — coroutine runs across multiple ticks
- `test_module_import` — import function from another file, call it
- `test_result_propagation` — `?` operator propagates error to caller
- `test_optional_chain` — `?.` returns null without crashing
- `test_when_exhaustive` — `when` over `Result<T>` handles both arms
- `test_host_type_integration` — register Rust type, call from script, verify result
- `test_sandbox_enforcement` — script cannot call unregistered function
- `test_instruction_limit_integration` — infinite loop script is terminated

**Verification:** `cargo test` (workspace) — all crates pass. A sample game-like script runs end-to-end.

**Spec refs:** All specs.

---

## Phase 17 — `writ-lsp` - Completed

**Why last in core:** The LSP reuses every crate above. It is pure tooling on top of the complete compiler pipeline.

**Deliverables:**

- `crates/writ-lsp/Cargo.toml` with LSP library dependency (`tower-lsp` or `lsp-server`)
- `Server` struct — LSP server lifecycle
- Handlers for all LSP methods from spec:
  - `textDocument/didOpen`, `didChange`, `didClose` — maintain per-file AST + type info
  - `textDocument/publishDiagnostics` — publish errors from type checker
  - `textDocument/completion` — completions from type environment at cursor position
  - `textDocument/definition` — go to definition via type checker name resolution
  - `textDocument/references` — find references via reference graph
  - `textDocument/hover` — type + doc comment at cursor
  - `textDocument/rename` — rename symbol and all references

**Tests:**

- `test_diagnostics_on_type_error`
- `test_completion_class_members`
- `test_completion_global_functions`
- `test_go_to_definition_local_var`
- `test_go_to_definition_imported_name`
- `test_hover_shows_type`
- `test_hover_shows_doc_comment`
- `test_rename_updates_all_references`
- `test_partial_parse_still_provides_completions`

**Verification:** `cargo test -p writ-lsp` — all tests pass. Manually verify VS Code picks up the server.

**Spec refs:** `.spec/tooling/lsp.md`

---

## Phase 18 — VS Code Extension - Completed

**Why this chunk:** The LSP is complete (Phase 17), but developers need a polished editor experience. The VS Code extension bundles the LSP client, syntax highlighting, debug adapter, and hot reload into a single installable package.

**Deliverables:**

- `vscode/` directory — VS Code extension project
- TextMate grammar (`syntaxes/writ.tmLanguage.json`) — syntax highlighting for all Writ constructs:
  - Keywords, operators, literals, comments, string interpolation
  - Scopes for classes, traits, enums, functions, parameters, type annotations
- Extension host (`extension.ts`):
  - LSP client lifecycle — start `writ-lsp` on activation, restart on crash
  - Configuration settings — LSP path, formatting options, diagnostics severity
  - Status bar indicator — LSP connection status
- DAP integration (Debug Adapter Protocol):
  - Launch configuration (`launch.json` schema) for Writ scripts
  - Breakpoint support — set/remove breakpoints via `VM::set_breakpoint`
  - Step over, step into, step out
  - Variable inspection in paused state
  - Call stack display
- Hot reload trigger:
  - File watcher on `.writ` files
  - On save → `VM::reload(path)` via LSP custom notification
  - Status bar notification on reload success/failure
- `package.json` with extension metadata, activation events, contributes

**Tests:**

- `test_tmgrammar_keywords_highlighted`
- `test_tmgrammar_string_interpolation`
- `test_tmgrammar_comments`
- `test_lsp_client_starts_on_activation`
- `test_lsp_client_restarts_on_crash`
- `test_dap_breakpoint_set_and_hit`
- `test_dap_step_over`
- `test_dap_variable_inspection`
- `test_hot_reload_on_save`
- `test_hot_reload_error_preserves_state`

**Verification:** Extension loads in VS Code, syntax highlighting works, breakpoints pause execution, hot reload updates running scripts.

**Spec refs:** `.spec/tooling/lsp.md`, `.spec/runtime/debug.md`

---

## Phase 19 — AoSoA Memory Layout (Mobile Only) - Completed

**Why this chunk:** Mobile platforms have strict memory and cache constraints. Array-of-Structs-of-Arrays layout improves cache coherence for batch operations on collections of game objects. Desktop targets use standard `Vec<Value>` unconditionally.

**Deliverables:**

- Cargo feature flag: `mobile-aosoa` — entire AoSoA codepath gated behind `#[cfg(feature = "mobile-aosoa")]`
- `AoSoA<T>` container — stores struct fields in interleaved SoA chunks
  - Configurable chunk size (default 64 elements per chunk for cache-line alignment)
  - `new(capacity: usize) -> AoSoA<T>`
  - `push(value: T)`, `get(index: usize) -> &T`
  - `iter() -> impl Iterator<Item = &T>`
  - `iter_field<F>(field: F) -> impl Iterator<Item = &F::Type>` — iterate a single field across all elements
- `Value::AoSoA(...)` variant — only present when feature is enabled
- Compiler support — `Array<T>` with `@packed` annotation compiles to AoSoA layout when feature is enabled; annotation is ignored on desktop (no error, just no-op)
- Batch operations — `map`, `filter`, `for_each` operate on contiguous field data
- Memory alignment utilities — cache-line aligned allocation
- Without the feature flag: standard `Vec<Value>` used everywhere, zero AoSoA code compiled

**Tests (`crates/writ-vm/tests/`):**

- `test_aosoa_push_and_get` — `#[cfg(feature = "mobile-aosoa")]`
- `test_aosoa_iterate_all` — `#[cfg(feature = "mobile-aosoa")]`
- `test_aosoa_iterate_single_field` — `#[cfg(feature = "mobile-aosoa")]`
- `test_aosoa_batch_map` — `#[cfg(feature = "mobile-aosoa")]`
- `test_aosoa_batch_filter` — `#[cfg(feature = "mobile-aosoa")]`
- `test_aosoa_chunk_boundary` — elements spanning multiple chunks — `#[cfg(feature = "mobile-aosoa")]`
- `test_aosoa_memory_layout` — verify field data is contiguous — `#[cfg(feature = "mobile-aosoa")]`
- `test_packed_annotation_ignored_without_feature` — `@packed` compiles and runs as standard `Vec<Value>`

**Verification:** `cargo test -p writ-vm` passes without feature. `cargo test -p writ-vm --features mobile-aosoa` passes with AoSoA tests included.

**Spec refs:** `.spec/runtime/vm.md`

---

## Phase 20 — Error Message Suggestions - Completed

**Why this chunk:** Good error messages are the difference between a frustrating and productive developer experience. Data-flow-aware suggestions go beyond simple edit distance to offer contextually relevant "did you mean?" hints.

**Deliverables:**

- `Suggestion` struct — `{ message: String, replacement: Option<String>, span: Span }`
- `TypeError` extended with `suggestions: Vec<Suggestion>`
- Edit-distance suggestions — for misspelled identifiers, type names, field names
  - Levenshtein distance with configurable threshold
  - Weighted scoring: prefer same-scope, same-type-category matches
- Data-flow suggestions:
  - "Did you mean to unwrap?" — when using `Result<T>` where `T` is expected
  - "Did you mean to call?" — when using a function name where its return type is expected
  - "This field is private" — when accessing a private field, suggest the public getter if one exists
  - "Missing `await`" — when using a coroutine result without `yield`
- Scope-aware suggestions — only suggest names visible in the current scope
- Type-aware suggestions — prefer suggestions that would fix the type error (e.g., suggest `.to_string()` when `string` expected but `int` found)

**Tests (`crates/writ-types/tests/`):**

- `test_suggest_misspelled_variable`
- `test_suggest_misspelled_type`
- `test_suggest_misspelled_field`
- `test_suggest_unwrap_result`
- `test_suggest_call_function`
- `test_suggest_public_getter_for_private_field`
- `test_suggest_type_conversion`
- `test_no_suggestion_when_nothing_close`
- `test_suggest_prefers_same_scope`
- `test_suggest_prefers_type_compatible`

**Verification:** `cargo test -p writ-types` — all tests pass. Error messages include actionable suggestions.

**Spec refs:** `.spec/language/type-system.md`, `.spec/tooling/lsp.md`
