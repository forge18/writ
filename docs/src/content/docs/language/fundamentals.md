---
title: Fundamentals
description: Variables, primitives, naming conventions, strings, operators, comments, and blocks.
---

## Variables

Three declaration keywords with distinct semantics:

| Keyword | Meaning                               |
|---------|---------------------------------------|
| `const` | Compile-time constant. Never changes. |
| `let`   | Runtime immutable. Assigned once.     |
| `var`   | Mutable. Can be reassigned.           |

```writ
const MAX_HEALTH = 100.0

let name = "Hero"           // type inferred from value
let name: string = "Hero"   // explicit type

var health = 100.0
health = 80.0               // ok
name = "Villain"            // compile error — let is immutable
```

Types are non-nullable by default. See [Error Handling](/writ/language/error-handling/) for `Optional<T>`.

---

## Primitive types

| Writ type | Description                                                            |
|-----------|------------------------------------------------------------------------|
| `int`     | Integer. Stored as i32, automatically promoted to i64 on overflow.     |
| `float`   | Floating point. Stored as f32, promoted to f64 when precision needs it.|
| `bool`    | `true` or `false`.                                                     |
| `string`  | UTF-8 text.                                                            |

The VM picks the smallest representation that fits and promotes transparently — scripts always see `int` and `float`.

```writ
let x: int = 42
let y: float = 3.15
let alive: bool = true
let name: string = "Hero"
```

---

## Naming conventions

| Kind                  | Convention | Example                      |
|-----------------------|------------|------------------------------|
| Primitives            | lowercase  | `int`, `float`, `string`     |
| User-defined types    | PascalCase | `Player`, `Entity`, `Weapon` |
| Methods and functions | camelCase  | `takeDamage`, `getHealth`    |
| Fields                | camelCase  | `maxHealth`, `currentSpeed`  |
| Enum variants         | PascalCase | `Direction.North`            |

---

## Strings

```writ
let name = "Hero"

// Interpolation
let greeting = "Hello, $name!"
let msg = "HP: ${player.health}"
let calc = "Total: ${a + b}"

// Escape the dollar sign
let literal = "Cost: \$50"

// Multi-line
let block = """
    Player: $name
    Health: $health
    """

// Concatenation
let full = "Hello" .. ", " .. name
```

---

## Operators

### Arithmetic

```writ
a + b    // add
a - b    // subtract
a * b    // multiply
a / b    // divide
a % b    // modulo
```

### Comparison

```writ
a == b   a != b
a < b    a > b
a <= b   a >= b
```

### Logical

```writ
a && b   // and
a || b   // or
!a       // not
```

### Assignment

```writ
x = 10
x += 5
x -= 2
x *= 3
x /= 2
x %= 4
```

### Other

| Operator    | Meaning                                          |
|-------------|--------------------------------------------------|
| `??`        | Null coalescing — use right side if left is null |
| `?.`        | Safe member access — short-circuits to null      |
| `?`         | Error propagation — return early on failure      |
| `..`        | String concatenation                             |
| `...`       | Spread into array or dict                        |
| `as`        | Type cast                                        |
| `a ? b : c` | Ternary                                          |

### Ranges

```writ
0..10    // exclusive: 0 to 9
0..=10   // inclusive: 0 to 10
```

---

## Comments

```writ
// Single line

/* Multi line
   comment */

// Doc comment — placed directly above a declaration
// The type checker and tooling pick these up automatically.
func takeDamage(amount: float) { }
```

---

## Blocks and semicolons

Curly braces delimit blocks. Semicolons are optional — newlines terminate statements. Unclosed brackets defer termination to their closing bracket:

```writ
let x = 10
let x = 10;   // also valid

let result = someFunction(
    arg1,
    arg2      // newline ignored inside parens
)
```
