# Type System

> **Crate:** `writ-types` | **Status:** Draft

## 1. Purpose

This spec defines Writ's type system — primitives, composite types, generics, traits, enums, nullability, error handling, and type inference. The type checker validates all types before bytecode is emitted. No runtime type checks occur.

## 2. Dependencies

| Depends On             | Relationship                                             |
|------------------------|----------------------------------------------------------|
| [syntax.md](syntax.md) | Type expressions appear throughout all syntax constructs |

**Depended on by:** [vm.md](../runtime/vm.md), [rust-interop.md](../runtime/rust-interop.md), [lsp.md](../tooling/lsp.md), [error-messages.md](../tooling/error-messages.md)

---

## 3. Primitive Types

Primitives are always lowercase. Numeric types use smart width promotion — they start at the narrowest representation and automatically promote when needed. Near-zero conversion cost at the FFI boundary.

| Type     | Rust Equivalent             | Description    |
|----------|-----------------------------|----------------|
| `int`    | `i32` (promotes to `i64`)   | Whole number   |
| `float`  | `f32` (promotes to `f64`)   | Decimal number |
| `bool`   | `bool`                      | True/false     |
| `string` | `String`                    | Text           |

### 3.1 Numeric Width Promotion

Numeric types are always signed. The compiler infers the narrowest concrete width from the initializer, and the runtime promotes automatically when a value outgrows its current width.

**Integer promotion:**
- Literal `42` → `i32` (fits in 32-bit signed)
- Literal `3_000_000_000` → `i64` (exceeds i32 range, inferred wide at compile time)
- Arithmetic overflow at runtime (e.g., `i32::MAX + 1`) → one-time promotion to `i64`

**Float promotion:**
- Literal `3.14` → `f32` (fits without precision loss)
- Value exceeds `f32` range at runtime → one-time promotion to `f64`

**Promotion rules:**
- Promotion is a one-time operation — once widened, the value stays at the wider type
- Mixed-width arithmetic promotes the narrower operand: `i32 + i64` → `i64`, `f32 + f64` → `f64`
- `int + float` → `float` (integer promoted to float)
- Host function signatures influence inference at call sites: a literal passed to a function expecting `i64` is inferred as `i64`

**FFI boundary behavior:**
- When the concrete width matches the host function parameter type → zero conversion cost
- When the value is wider than the host expects → range check (one comparison, essentially free)
- When a host function expects an unsigned type (`u32`, `u64`) → range check that the signed value is non-negative

---

## 4. Composite Types

### 4.1 Optional\<T\>

Represents a value that may or may not be present. Types are non-nullable by default.

```writ
let nickname: Optional<string>     // may have no value
let name: string = "Hero"          // never null
```

**Operations:**

- `.hasValue` — bool property, true if a value is present
- `??` — null coalescing, provides a fallback
- `?.` — safe member access, returns `Optional<T>` of the accessed member

```writ
let n = nickname ?? "Unknown"
let length = nickname?.length
```

The type checker narrows the type inside `if nickname.hasValue { }` blocks — no explicit unwrap needed.

### 4.2 Result\<T\>

Represents a computation that can succeed or fail. Errors are always string messages.

```writ
func divide(a: float, b: float) -> Result<float> {
    if b == 0 {
        return Error("Division by zero")
    }
    return Success(a / b)
}
```

**Operations:**

- `is Success(value)` — pattern match on success, binds `value`
- `is Error(msg)` — pattern match on failure, binds `msg`
- `?` — propagate error up the call stack (early return on `Error`)
- `??` — fallback value on error

```writ
let value = divide(10, 0)?    // propagate
let value = divide(10, 0) ?? 0.0  // fallback
```

### 4.3 Collections

| Type               | Rust Equivalent | Description                    |
|--------------------|-----------------|--------------------------------|
| `Array<T>`         | `Vec<T>`        | Ordered, typed, indexed by int |
| `Dictionary<K, V>` | `HashMap<K, V>` | Key-value, typed               |

Generics compose freely:

```writ
Array<Dictionary<string, Player>>
Dictionary<string, Array<int>>
```

Traits can be used as Dictionary value types for heterogeneous structured data.

### 4.4 Tuples

Rust-style fixed-length, fixed-type collections. Destructuring supported.

```writ
let point: (float, float) = (10.0, 20.0)
let (x, y) = point
```

---

## 5. Generics

Standard `<>` syntax. Type parameters are resolved at compile time.

```writ
Array<string>
Dictionary<string, int>
Optional<float>
Result<float>
```

User-defined generic classes and structs are supported via monomorphization at type-check time.

```writ
struct Pair<A, B> {
    first: A
    second: B
}

class Stack<T> {
    top: T
}
```

Each unique instantiation (e.g. `Stack<int>`, `Stack<string>`) is treated as a distinct concrete type by the compiler and VM. Generic templates themselves are not compiled — only their monomorphic instantiations are.

### 5.1 Generic Constraints (`where` clauses)

Type parameters can be constrained to require a trait implementation using `where` clauses. This allows calling trait methods on generic type parameters inside the function or class body.

```writ
trait Printable {
    func print()
}

func printAll<T>(item: T) where T : Printable {
    item.print()
}

class Container<T> where T : Printable {
    value: T

    func show() {
        value.print()
    }
}
```

- Multiple constraints are separated by commas: `where T : Trait1, U : Trait2`
- Constraints are validated at type-check time; the type checker enforces that each named trait exists

---

## 6. Traits

Replace both interfaces and abstract classes. May have default implementations.

```writ
trait Damageable {
    func takeDamage(amount: float)   // no default — must be implemented

    func die() {                     // default — can be overridden
        print("Entity died")
    }
}
```

**Rules:**

- A class implements a trait by declaring `with TraitName`
- All non-default methods must be implemented — compile error otherwise
- Default methods are inherited unless overridden
- Multiple traits allowed: `with Damageable, Updatable`
- Traits can be used as parameter types and dictionary value types

---

## 7. Structs

Value-type data containers. Copied on assignment. No inheritance, no traits.

```writ
struct Point {
    public x: float = 0.0
    public y: float = 0.0
}

var a = Point(x: 1.0, y: 2.0)
var b = a           // b is a copy
b.x = 99.0         // a.x is still 1.0
```

**Rules:**

- Structs are value types — assignment copies the entire struct
- Structural equality — two structs are equal if all fields are equal
- No `extends` — structs cannot inherit
- No `with` — structs cannot implement traits
- Fields, methods, setters, static methods, and auto-generated constructors work identically to classes

See [structs.md](structs.md) for full specification.

---

## 8. Enums

Java-style — can have fields and methods.

```writ
// Simple
enum Direction {
    North, South, East, West
}

// With fields and methods
enum Status {
    Alive(100), Dead(0), Wounded(50)

    health: int

    func getHealth() -> int {
        return health
    }
}
```

Enum variants are accessed via dot notation: `Direction.North`.

---

## 8. Type Inference

The type checker infers types from literals when unambiguous:

```writ
let name = "Hero"        // inferred: string
let health = 100.0       // inferred: float
let items = ["a", "b"]  // inferred: Array<string>
```

Explicit annotation required when:

- The inferred type would be ambiguous
- A wider type is intended (`Array<Damageable>` when assigning `Array<Player>`)
- The variable is declared without initialization

---

## 9. Naming Conventions

| Kind                  | Convention | Example                     |
|-----------------------|------------|-----------------------------|
| Primitives            | lowercase  | `int`, `float`, `string`    |
| User-defined types    | PascalCase | `Player`, `Weapon`          |
| Methods and functions | camelCase  | `takeDamage`, `getHealth`   |
| Fields                | camelCase  | `maxHealth`, `currentSpeed` |
| Enum variants         | PascalCase | `Direction.North`           |

---

## 10. Visibility

Private by default. `public` to expose, `private` optional for explicitness.

```writ
class Player {
    public name: string
    private speed: float = 5.0
    weapons: Array<Weapon> = []  // implicitly private
}
```

---

## 11. Type Checking Rules

1. All variables must have a resolved type before use
2. Assignment target type must match the value type — no implicit coercion
3. `Optional<T>` and `T` are distinct types — cannot assign one to the other without explicit handling
4. Trait method implementations must match the declared signature exactly
5. `when` expressions over enums or `Result<T>` must be exhaustive — compiler error otherwise
6. `?` operator is only valid in functions that return `Result<T>`
7. Return type `void` and absent return type are equivalent

---

## 12. Edge Cases

1. **Given** a variable declared as `string` is assigned `null`, **then** compile error — `string` is non-nullable.
2. **Given** a `when` expression over a `Result<T>` is missing the `is Error` arm, **then** compile error — exhaustiveness required.
3. **Given** a trait method with a default implementation is not overridden, **then** the default is used — no error.
4. **Given** a function body falls off the end without a `return` on a branch, **then** compile error if return type is non-void.
5. **Given** `?` is used in a function returning `void`, **then** compile error — `?` requires `Result<T>` return type.

---

## 13. Revision History

| Date       | Change                                                          |
|------------|-----------------------------------------------------------------|
| 2026-03-03 | Simplified numeric types to `int`/`float` with smart promotion  |
| 2026-03-02 | Initial draft                                                   |
