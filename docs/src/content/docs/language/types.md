---
title: Types
description: Structs, classes, traits, enums, generics, optionals, and results.
---

## Structs

Value types — copied on assignment, compared by value. Good for small data with no identity (vectors, colors, rectangles).

```writ
struct Vec2 {
    x: float
    y: float

    func length() -> float {
        return (x * x + y * y).sqrt()
    }

    func add(other: Vec2) -> Vec2 {
        return Vec2(x: x + other.x, y: y + other.y)
    }
}

let a = Vec2(x: 3.0, y: 4.0)
let b = a               // copied — a and b are independent
print(a.length())       // 5.0
```

Fields without defaults must be provided at construction. Fields with defaults are optional:

```writ
struct Rect {
    x: float = 0.0
    y: float = 0.0
    width: float
    height: float
}

let r = Rect(width: 100.0, height: 50.0)   // x and y default to 0
```

---

## Classes

Reference types — assigned by reference, identity-based. Good for game objects with state and lifecycle.

```writ
class Entity {
    public id: int
    public name: string
}

class Player extends Entity {
    public health: float = 100.0
    private speed: float = 5.0

    public func takeDamage(amount: float) {
        health -= amount
        if health <= 0 { die() }
    }
}
```

### Visibility

Private by default. `public` to expose, `private` for explicitness.

```writ
class Player {
    public name: string     // accessible from scripts and host
    private speed: float    // internal only
    weapons: Array<Weapon>  // implicitly private
}
```

### Constructors

Auto-generated from field declarations — no boilerplate needed. Write a custom constructor only when special logic is required.

```writ
let p1 = Player()                             // default values
let p2 = Player(name: "Hero", health: 80.0)   // memberwise
```

### Getters and setters

Declare a field normally; add accessor logic only when needed:

```writ
public health: float = 100.0
    set(value) { field = clamp(value, 0.0, maxHealth) }
```

`field` refers to the backing value inside the setter.

### Inheritance

Single inheritance via `extends`. `self` is implicit inside methods.

```writ
class Boss extends Enemy {
    public phase: int = 1

    public func takeDamage(amount: float) {
        health -= amount * 0.5   // bosses take half damage
    }
}
```

### Static methods

```writ
class Player {
    public static func create(name: string) -> Player {
        return Player(name: name, health: 100.0)
    }
}

let p = Player.create("Hero")
```

---

## Traits

Replace both interfaces and abstract classes. Can have required methods and default implementations.

```writ
trait Damageable {
    func takeDamage(amount: float)   // required — must implement

    func die() {                     // default — can override
        print("Entity died")
    }
}

trait Updatable {
    func update(delta: float)
}
```

Apply multiple traits with `with`:

```writ
class Player extends Entity with Damageable, Updatable {
    func takeDamage(amount: float) {
        health -= amount
    }

    func update(delta: float) {
        move(delta)
    }

    // die() inherited from Damageable default
}
```

Traits can be used as value types for heterogeneous collections:

```writ
let units: Array<Damageable> = [player, enemy, boss]
for unit in units {
    unit.takeDamage(10.0)
}
```

---

## Enums

Java-style — can have fields, methods, and be pattern matched.

```writ
// Simple
enum Direction { North, South, East, West }

// With data and methods
enum Status {
    Alive(100), Wounded(50), Dead(0)

    health: int

    func isAlive() -> bool {
        return health > 0
    }
}

let dir = Direction.North
let status = Status.Wounded
```

Pattern match on enums with `when`:

```writ
when dir {
    is Direction.North => moveNorth()
    is Direction.South => moveSouth()
    is Direction.East  => moveEast()
    is Direction.West  => moveWest()
}
```

---

## Generics

Standard `<T>` syntax. Compose freely:

```writ
func first<T>(items: Array<T>) -> Optional<T> {
    if items.isEmpty() { return null }
    return items[0]
}

let names: Array<string> = ["Alice", "Bob"]
let name = first(names)   // Optional<string>
```

Built-in generic types:

```writ
Array<string>
Dictionary<string, int>
Optional<Player>
Result<float>
Array<Dictionary<string, Player>>
```

---

## Optionals

Types are non-nullable by default. Wrap in `Optional<T>` to allow absence.

```writ
let target: Optional<Player>   // may be absent

// Check before using
if target.hasValue {
    target.takeDamage(10.0)
}

// Safe member access — short-circuits to null if absent
let hp = target?.health

// Null coalescing — use fallback if absent
let hp = target?.health ?? 0.0

// Unwrap with fallback
let name = target?.name ?? "Nobody"
```

---

## Results

Functions that can fail return `Result<T>`. Errors are always string messages.

```writ
func divide(a: float, b: float) -> Result<float> {
    if b == 0.0 {
        return Error("Division by zero")
    }
    return Success(a / b)
}
```

Handle results with `when`:

```writ
when divide(10.0, 2.0) {
    is Success(value) => print(value)
    is Error(msg)     => print("Failed: " .. msg)
}
```

Propagate errors up with `?`:

```writ
func calculate(a: float, b: float) -> Result<float> {
    let quotient = divide(a, b)?   // returns early on error
    return Success(quotient * 2.0)
}
```

Use `??` for a fallback value instead of propagating:

```writ
let result = divide(10.0, 0.0) ?? 0.0
```

---

## Tuples

Rust-style, with destructuring:

```writ
let point = (10.0, 20.0)
let (x, y) = point

func getPosition() -> (float, float) {
    return (self.x, self.y)
}

let (px, py) = player.getPosition()
```

---

## Collections

```writ
// Arrays — ordered, typed
let items: Array<string> = ["sword", "shield", "potion"]
let grid: Array<Array<int>>

items.push("dagger")
let first = items[0]

// Dictionaries — key-value, typed
let scores: Dictionary<string, int> = {"alice": 100, "bob": 95}
scores["carol"] = 88
let aliceScore = scores["alice"]

// Spread
let combined = [...items1, ...items2]
let merged   = {...dict1, ...dict2}
```

---

## Type casting

```writ
let n = 3.9 as int      // 3
let f = 42 as float     // 42.0
```
