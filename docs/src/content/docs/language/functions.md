---
title: Functions
description: Named functions, return types, arguments, variadic parameters, and lambdas.
---

## Named functions

```writ
func takeDamage(amount: float) {
    health -= amount
}

func divide(a: float, b: float) -> float {
    return a / b
}
```

Void return can be implicit or explicit:

```writ
func doSomething() { }
func doSomething() -> void { }
```

---

## Arguments

Named and positional arguments both work at the call site:

```writ
damage(target: enemy, amount: 50.0)  // named
damage(enemy, 50.0)                  // positional
```

---

## Variadic parameters

Use `...` to accept a variable number of arguments:

```writ
func sum(...numbers: int) -> int {
    var total = 0
    for n in numbers { total += n }
    return total
}

sum(1, 2, 3, 4)  // 10
```

---

## Static methods

```writ
class Player {
    public static func create(name: string) -> Player {
        return Player(name: name, health: 100.0)
    }
}

let p = Player.create("Hero")
```

---

## Lambdas

Parameter types required; return type is inferred.

```writ
let double = (x: int) => x * 2

let onDamage = (amount: float) => {
    print("Took " .. amount .. " damage")
    health -= amount
}
```

Lambdas can be reassigned when declared with `var`:

```writ
var handler = (amount: float) => {
    print(amount)
}
```

`return` inside a lambda returns from the lambda only, never the enclosing function.
