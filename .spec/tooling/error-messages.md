# Error Messages

> **Crate:** `writ-types`, `writ-compiler` | **Status:** Draft

## 1. Purpose

This spec defines the format, content, and rules for Writ's compiler error messages. Writ follows the standard set by Rust and Elm — errors are precise, plain English, and actionable. Good error messages are a first-class feature of the language.

## 2. Dependencies

| Depends On                                   | Relationship                                    |
|----------------------------------------------|-------------------------------------------------|
| [type-system.md](../language/type-system.md) | Most errors originate in the type checker       |
| [syntax.md](../language/syntax.md)           | Parse errors originate in the parser            |
| [lsp.md](lsp.md)                             | LSP surfaces these errors as inline diagnostics |

---

## 3. Error Format

Every error includes:

- A short plain-English description of what went wrong
- The file path, line number, and column number
- A source excerpt showing surrounding lines
- A caret (`^`) pointing to the exact token or expression
- A suggestion where the type checker can infer one

```text
Error: Type mismatch
  --> entities/player.writ:34:12
   |
32 |     public func takeDamage(amount: float) {
33 |         health = "full"
   |                  ^^^^^^ expected float, found string
34 |     }
```

```text
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

---

## 4. Format Rules

### 4.1 Header

```
Error: <plain English description>
```

One sentence. No jargon. Describes what went wrong, not the internal mechanism.

**Good:** `Type mismatch`
**Bad:** `TypeError: incompatible types in assignment expression`

### 4.2 Location

```
  --> file/path.writ:line:column
```

Always present. Path is relative to the project root.

### 4.3 Source Excerpt

```
   |
N  |     source line
   |     ^^^^^^ annotation
N+1|     next line
   |
```

- Show the problem line plus one line of context above and below where helpful
- The caret spans the exact token or expression that caused the error
- The annotation after the caret describes the specific issue in a few words

### 4.4 Suggestions

```
   |
   = did you mean 'health'?
   = help: add a return type annotation: -> float
   = note: 'Result<T>' requires '?' to be in a function returning 'Result'
```

Suggestions are prefixed with `=`. They are offered when the type checker can confidently infer what the developer intended.

**Suggestion triggers:**

- Unknown field or method: suggest similarly named fields/methods (edit distance ≤ 2)
- Wrong type: suggest a cast when the conversion is lossless (`int` → `float`)
- Missing `?`: suggest adding it when `Result<T>` is used in a non-result context
- Missing return type: suggest the inferred return type
- Unused variable: suggest prefixing with `_` to suppress

---

## 5. Error Categories

### 5.1 Parse Errors

Produced by `writ-parser`. Emitted when source code does not match the grammar.

```
Error: Expected closing brace
  --> scripts/enemy.writ:24:1
   |
22 |     func update(delta: float) {
23 |         position.x += speed * delta
24 |
   | ^ unexpected end of file, expected '}' |
   |----------------------------------------|
   = note: opening brace is at line 22
```

### 5.2 Type Errors

Produced by `writ-types`. The most common category.

**Type mismatch:**

```
Error: Type mismatch
  --> entities/player.writ:12:18
   |
12 |     let count: int = 3.14
   | ^^^^ expected int, found float |
   |--------------------------------|
   = help: cast with 'as int' to truncate: 3.14 as int
```

**Missing return:**

```
Error: Missing return value
  --> math/utils.writ:8:1
   |
 5 |     func add(a: int, b: int) -> int {
 6 |         if a > 0 {
 7 |             return a + b
 8 |         }
   | ^ this branch does not return a value |
   |---------------------------------------|
   = note: all branches must return 'int'
```

**Unknown name:**

```
Error: Unknown variable 'helth'
  --> entities/player.writ:15:9
   |
15 |         helth -= amount
   | ^^^^^ undefined variable |
   |--------------------------|
   = did you mean 'health'?
```

**Non-exhaustive when:**

```
Error: Non-exhaustive pattern match
  --> combat/damage.writ:30:5
   |
30 |     when result {
   | ^^^^ missing arm for 'Error' |
   |------------------------------|
   = help: add 'is Error(msg) => ...' or 'else => ...'
```

**Nullable without Optional:**

```
Error: Type 'string' is not nullable
  --> entities/player.writ:5:25
   |
 5 |     let name: string = null
   | ^^^^ 'string' cannot be null |
   |------------------------------|
   = help: use 'Optional<string>' to allow absence of a value
```

### 5.3 Module Errors

```
Error: Module not found
  --> scripts/player.writ:1:28
   |
 1 |     import { Weapon } from "items/wepon"
   | ^^^^^^^^^^^^ no module at this path |
   |-------------------------------------|
   = did you mean 'items/weapon'?
```

```
Error: Name not exported
  --> scripts/player.writ:1:14
   |
 1 |     import { weapon } from "items/weapon"
   | ^^^^^^ 'weapon' is not exported from this module |
   |--------------------------------------------------|
   = note: exported names are: Weapon, createWeapon
```

---

## 6. Rules

1. Every error has a file, line, and column — no "somewhere in this file" errors
2. Plain English — describe what went wrong, not the type system mechanism
3. Show surrounding lines — one line of context above and below the problem
4. Caret points to the exact token — not the whole line
5. Suggest fixes when the type checker can confidently infer intent
6. Avoid cascading errors — if a type error on line 5 would cause 20 follow-on errors, suppress the follow-ons and show only the root cause
7. Errors are structured data — the host receives them as typed structs, not just formatted strings

---

## 7. Error Data Structure

```rust
pub struct CompileError {
    pub message: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub span_length: u32,
    pub source_context: Vec<SourceLine>,
    pub annotation: String,
    pub suggestions: Vec<String>,
}

pub struct SourceLine {
    pub line_number: u32,
    pub content: String,
    pub is_error_line: bool,
}
```

---

## 8. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
