# Syntax

> **Crate:** `writ-parser` | **Status:** Draft

## 1. Purpose

This spec defines Writ's complete syntax — variables, functions, classes, traits, enums, control flow, coroutines, modules, strings, operators, and comments.

## 2. Dependencies

| Depends On                       | Relationship         |
|----------------------------------|----------------------|
| [type-system.md](type-system.md) | All type expressions |

**Depended on by:** [vm.md](../runtime/vm.md), [lsp.md](../tooling/lsp.md)

---

## 3. Blocks and Statements

- Curly braces `{}` delimit blocks
- Semicolons are optional — newlines terminate statements
- Unclosed brackets or parentheses defer termination to the closing bracket

```writ
let x = 10
let x = 10;  // both valid

let result = someFunction(
    argument1,
    argument2
)  // multi-line expression — parser waits for closing paren
```

---

## 4. Variable Declaration

| Keyword | Meaning                              |
|---------|--------------------------------------|
| `const` | Compile-time constant, never changes |
| `let`   | Runtime immutable, set once          |
| `var`   | Mutable, can be reassigned           |

```writ
const MAX_HEALTH = 100.0
let name: string = "Hero"
let name = "Hero"         // inferred
var health: float = 100.0
var health = 100.0        // inferred
```

---

## 5. Functions

```writ
// Basic
func takeDamage(amount: float) {
    health -= amount
}

// With return type
func divide(a: float, b: float) -> Result<float> {
    return Success(a / b)
}

// Void — both forms valid
func doSomething() { }
func doSomething() -> void { }

// Static method
static func create(name: string) -> Player {
    return Player(name: name)
}

// Named and positional parameters both valid at call site
let p1 = Player(name: "Hero", health: 80.0)
let p2 = Player("Hero", 80.0)

// Variadic
func sum(...numbers: int) -> int {
    var total = 0
    for n in numbers {
        total += n
    }
    return total
}
```

### 5.1 Lambdas

Parameter types required. Return type always inferred. `return` exits the lambda only, never the enclosing function.

```writ
let double = (x: float) => x * 2

let onDamage = (amount: float) => {
    print("Took " .. amount .. " damage")
}

var handler = (amount: float) => {
    print(amount)
}
```

---

## 6. Classes

Single inheritance via `extends`, multiple traits via `with`.

```writ
class Player extends Entity with Damageable, Updatable {
    public name: string
    public health: float = 100.0
        set(value) { field = clamp(value, 0.0, 100.0) }

    private speed: float = 5.0
    weapons: Array<Weapon> = []

    public static func create(name: string) -> Player {
        return Player(name: name)
    }

    public func takeDamage(amount: float) {
        health -= amount
        if health <= 0 {
            die()
        }
    }
}
```

### 6.1 Constructors

Default and memberwise constructors are auto-generated from field declarations. Write a custom constructor only when special initialization logic is needed.

```writ
let p1 = Player()
let p2 = Player(name: "Hero", health: 80.0)
```

### 6.2 Getters and Setters

Declare the field normally. Add accessor logic only when needed. `field` refers to the backing storage inside the setter.

```writ
public health: float = 100.0
    set(value) { field = clamp(value, 0.0, maxHealth) }
```

### 6.3 self

`self` is implicitly available inside instance methods. Explicit `self.` is optional.

```writ
func takeDamage(amount: float) {
    self.health -= amount  // explicit
    health -= amount       // also valid
}
```

---

## 7. Traits

```writ
trait Damageable {
    func takeDamage(amount: float)  // no default

    func die() {                    // default
        print("Entity died")
    }
}
```

See [type-system.md §6](type-system.md) for full trait rules.

---

## 8. Structs

Value-type data containers. No inheritance, no traits. See [structs.md](structs.md) for full semantics.

```writ
struct Point {
    public x: float = 0.0
    public y: float = 0.0

    func length() -> float {
        return sqrt(x * x + y * y)
    }
}

let p = Point(x: 10.0, y: 20.0)
```

---

## 9. Enums

```writ
enum Direction {
    North, South, East, West
}

enum Status {
    Alive(100), Dead(0), Wounded(50)

    health: int

    func getHealth() -> int {
        return health
    }
}
```

---

## 9. Control Flow

### 9.1 if / else

```writ
if health <= 0 {
    die()
} else if health < 25 {
    playLowHealthSound()
} else {
    heal()
}
```

### 9.2 Ternary

```writ
let status = health > 0 ? "alive" : "dead"
```

### 9.3 when

Kotlin-style pattern matching. Exhaustive — compiler warns if not all cases are handled.

```writ
// Value matching
when health {
    0 => print("Dead")
    100 => print("Full health")
    else => print("Damaged")
}

// Multiple values per arm
when health {
    0, 1, 2 => print("Critical")
    else => print("OK")
}

// Range matching
when health {
    0..25 => print("Critical")
    26..=100 => print("OK")
}

// Type matching with binding
when result {
    is Success(value) => print(value)
    is Error(msg) => print(msg)
}

// Guard clauses
when health {
    x if x < 0 => print("Invalid")
    x if x < 25 => print("Critical: $x")
    else => print("OK")
}

// Multi-line arms
when result {
    is Success(value) => {
        print(value)
        log(value)
    }
    is Error(msg) => print(msg)
}

// Without subject — replaces if/else chains
when {
    health == 100 => print("Full health")
    health <= 0 => print("Dead")
    else => print("Damaged")
}
```

### 9.4 Loops

```writ
while health > 0 {
    update()
}

for item in items {
    print(item)
}

for i in 0..10 {    // exclusive: 0 to 9
    print(i)
}

for i in 0..=10 {   // inclusive: 0 to 10
    print(i)
}

break
continue
```

---

## 10. Strings

```writ
let greeting = "Hello $name"
let message = "Health: ${player.health}"
let calc = "Result: ${a + b}"
let literal = "Hello \$name"    // escape interpolation

let message = """
    Player $name has $health health.
    Located at ${position.x}, ${position.y}.
    """

let full = "Hello" .. " " .. name  // concatenation
```

---

## 11. Collections

```writ
let items: Array<string> = ["sword", "shield"]
let items = ["sword", "shield"]              // inferred
let grid: Array<Array<int>>

let scores: Dictionary<string, int> = {"alice": 100, "bob": 95}

let combined = [...array1, ...array2]        // spread
let merged = {...dict1, ...dict2}
```

---

## 12. Tuples

```writ
let point = (10.0, 20.0)
let point: (float, float) = (10.0, 20.0)

let (x, y) = point   // destructuring

func getPosition() -> (float, float) {
    return (x, y)
}

let (x, y) = getPosition()
```

---

## 13. Coroutines

Any function containing `yield` is implicitly a coroutine. See [coroutines.md](../runtime/coroutines.md) for full semantics.

```writ
func openDoor() {
    playAnimation("door_open")
    yield waitForSeconds(2.0)
    setCollider(false)
}

start openDoor()

yield
yield waitForSeconds(5.0)
yield waitForFrames(10)
yield waitUntil(() => isReady)
yield openDoor()

func getInput() -> string {
    yield waitForKeyPress()
    return lastKey
}

let key = yield getInput()
```

---

## 14. Modules

```writ
// weapon.writ — exporting
export class Weapon {
    public damage: float = 10.0
}

export func createWeapon(damage: float) -> Weapon {
    return Weapon(damage: damage)
}

// player.writ — importing
import { Weapon, createWeapon } from "items/weapon"
import * as enemy from "entities/enemy"

func attack(target: enemy::Enemy) {
    weapon.attack(target)
}
```

**Rules:**

- Named exports only — no default exports
- Host/engine types always globally available, never imported
- `::` for namespace access on wildcard imports

---

## 15. Operators

### Arithmetic

```
+ - * / %
```

### Comparison

```
== != < > <= >=
```

### Logical

```
&& || !
```

### Assignment

```
= += -= *= /= %=
```

### Other

| Operator | Meaning              |
|----------|----------------------|
| `??`     | Null coalescing      |
| `?.`     | Safe member access   |
| `?`      | Error propagation    |
| `..`     | String concatenation |
| `...`    | Spread               |
| `as`     | Type casting         |
| `? :`    | Ternary              |

### Ranges

| Syntax   | Meaning             |
|----------|---------------------|
| `0..10`  | Exclusive (0 to 9)  |
| `0..=10` | Inclusive (0 to 10) |

Operator overloading uses a named-method convention. Define methods with the following names on a class or struct to overload the corresponding operator:

| Operator | Method name |
|----------|-------------|
| `+`      | `add`       |
| `-`      | `subtract`  |
| `*`      | `multiply`  |
| `/`      | `divide`    |
| `%`      | `modulo`    |
| `<`      | `lt`        |
| `<=`     | `le`        |
| `>`      | `gt`        |
| `>=`     | `ge`        |

```writ
struct Vec2 {
    x: float
    y: float
    func add(other: Vec2) -> Vec2 {
        return Vec2(self.x + other.x, self.y + other.y)
    }
}

let a = Vec2(1.0, 2.0)
let b = Vec2(3.0, 4.0)
let c = a + b  // calls a.add(b)
```

The method is dispatched at runtime. If no matching method is found, a runtime error is produced.

---

## 16. Type Casting

```writ
let x = someValue as float
let n = health as int
```

---

## 17. Comments

Go-style doc comments — plain `//` directly above a declaration. No special syntax required.

```writ
// Calculates damage dealt to the player.
// Takes armor into account before applying to health.
func takeDamage(amount: float) -> Result<bool> {
    health -= amount
    return Success(health <= 0)
}

// Single line comment
/* Multi line comment */
```

---

## 18. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
