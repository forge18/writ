---
title: Traits & Enums
description: Shared behavior with traits and fixed sets of values with enums.
---

## Traits

Traits define shared behavior. They replace both interfaces and abstract classes — a trait can have required methods (no body) and default implementations (with a body).

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

### Implementing traits

Apply traits to a class with `with`:

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

### Traits as types

Traits can be used as value types for heterogeneous collections:

```writ
let units: Array<Damageable> = [player, enemy, boss]
for unit in units {
    unit.takeDamage(10.0)
}
```

---

## Enums

Fixed sets of named values. Can be simple labels or carry associated data and methods.

### Simple enums

```writ
enum Direction { North, South, East, West }

let dir = Direction.North
```

### Enums with data

Variants can carry values. Declare a field and pass values in parentheses:

```writ
enum Status {
    Alive(100), Wounded(50), Dead(0)

    health: int

    func isAlive() -> bool {
        return health > 0
    }
}

let status = Status.Wounded
print(status.isAlive())   // true
```

### Pattern matching

Use `when` to branch on enum variants:

```writ
when dir {
    is Direction.North => moveNorth()
    is Direction.South => moveSouth()
    is Direction.East  => moveEast()
    is Direction.West  => moveWest()
}
```
