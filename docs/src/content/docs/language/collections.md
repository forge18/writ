---
title: Collections
description: Arrays, dictionaries, tuples, and spread syntax.
---

## Arrays

Ordered, typed collections indexed by integer:

```writ
let items: Array<string> = ["sword", "shield", "potion"]
let grid: Array<Array<int>>

items.push("dagger")
let first = items[0]
```

### Spread

Combine arrays with `...`:

```writ
let combined = [...items1, ...items2]
```

---

## Dictionaries

Key-value collections, typed:

```writ
let scores: Dictionary<string, int> = {"alice": 100, "bob": 95}
scores["carol"] = 88
let aliceScore = scores["alice"]
```

### Spread

Merge dictionaries with `...`:

```writ
let merged = {...dict1, ...dict2}
```

### Nested collections

Generics compose freely:

```writ
let teams: Array<Dictionary<string, Player>>
let nested: Dictionary<string, Dictionary<string, int>>
```

---

## Tuples

Rust-style, with destructuring:

```writ
let point = (10.0, 20.0)
let (x, y) = point
```

### Typed tuples

```writ
let point: (float, float) = (10.0, 20.0)
let player = ("Hero", 100.0, true)
let (name, health, isActive) = player
```

### Function returns

Tuples are useful for returning multiple values:

```writ
func getPosition() -> (float, float) {
    return (self.x, self.y)
}

let (px, py) = player.getPosition()
```
