---
title: Classes & Structs
description: Reference types vs value types — when to use each, and how they work.
---

Writ has two ways to define custom types: **classes** (reference types) and **structs** (value types). The difference matters for how data moves through your program.

## When to use which

| Use a **class** when... | Use a **struct** when... |
|---|---|
| The object has identity (a specific player, enemy, or UI element) | The data is a value with no identity (a position, color, or rectangle) |
| You need inheritance or traits | You just need a data container with optional methods |
| Other code should see the same instance when you pass it around | Copies should be independent — changing one shouldn't affect another |

**Rule of thumb:** if it's a "thing" in your game, it's a class. If it's a measurement or coordinate, it's a struct.

---

## Structs

Copied on assignment, compared by value.

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

Structs cannot inherit from other types or implement traits.

---

## Classes

Assigned by reference. When you pass a class instance, every variable points to the same object.

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

Fields and methods are **private by default**. Mark them `public` to expose them.

```writ
class Player {
    public name: string     // accessible everywhere
    private speed: float    // explicit private
    weapons: Array<Weapon>  // also private (default)
}
```

### Constructors

Auto-generated from field declarations. No boilerplate needed.

```writ
let p1 = Player()                             // default values
let p2 = Player(name: "Hero", health: 80.0)   // memberwise
```

### Getters and setters

Declare the field normally. Add accessor logic only when needed:

```writ
public health: float = 100.0
    set(value) { field = clamp(value, 0.0, maxHealth) }
```

`field` refers to the backing value inside the setter.

### Inheritance

Single inheritance via `extends`. Apply traits with `with`.

```writ
class Boss extends Enemy {
    public phase: int = 1

    public func takeDamage(amount: float) {
        health -= amount * 0.5   // bosses take half damage
    }
}
```

### self and super

`self` is available implicitly inside instance methods. `super.method()` calls the parent's implementation.

```writ
class Dog extends Animal {
    func speak() -> string {
        let base = super.speak()
        return base + " Woof!"
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
