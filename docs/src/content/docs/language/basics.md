---
title: Language Basics
description: Variables, functions, control flow, strings, and operators.
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

Types are non-nullable by default. See [Types](/writ/language/types) for `Optional<T>`.

---

## Primitive types

| Writ type  | Rust equivalent |
|------------|-----------------|
| `int`      | `i32`           |
| `bigint`   | `i64`           |
| `float`    | `f32`           |
| `bigfloat` | `f64`           |
| `bool`     | `bool`          |
| `string`   | `String`        |

```writ
let x: int = 42
let y: float = 3.14
let alive: bool = true
let name: string = "Hero"
```

---

## Functions

```writ
func takeDamage(amount: float) {
    health -= amount
}

func divide(a: float, b: float) -> float {
    return a / b
}
```

Named and positional arguments both work at the call site:

```writ
damage(target: enemy, amount: 50.0)  // named
damage(enemy, 50.0)                  // positional
```

Variadic parameters use `...`:

```writ
func sum(...numbers: int) -> int {
    var total = 0
    for n in numbers { total += n }
    return total
}

sum(1, 2, 3, 4)  // 10
```

### Lambdas

Parameter types required; return type is inferred.

```writ
let double = (x: int) => x * 2

let onDamage = (amount: float) => {
    print("Took " .. amount .. " damage")
    health -= amount
}
```

`return` inside a lambda returns from the lambda only, never the enclosing function.

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

## Control flow

### if / else

```writ
if health <= 0 {
    die()
} else if health < 25 {
    playLowHealthWarning()
} else {
    heal()
}
```

### Ternary

```writ
let status = health > 0 ? "alive" : "dead"
```

### when

Pattern matching with exhaustiveness checking. Two forms:

```writ
// With a subject — match on its value
when health {
    0          => print("Dead")
    1, 2, 3    => print("Critical")
    0..25      => print("Low")
    26..=100   => print("OK")
    else       => print("Overheal")
}

// Type matching (with Result/Optional)
when result {
    is Success(value) => print(value)
    is Error(msg)     => print("Error: " .. msg)
}

// Guard clauses
when health {
    x if x < 0    => print("Invalid")
    x if x < 25   => print("Critical: $x")
    else           => print("OK")
}

// Without a subject — replaces if/else chains
when {
    health == 100 => print("Full")
    health <= 0   => print("Dead")
    else          => print("Damaged")
}
```

### Loops

```writ
// while
while health > 0 {
    tick()
}

// for over a collection
for item in inventory {
    print(item.name)
}

// for over a range
for i in 0..10 {
    print(i)   // 0 to 9
}

break     // exit loop
continue  // skip to next iteration
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
