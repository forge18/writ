---
title: Generics & Casting
description: Type parameters and type conversions.
---

## Generics

Use `<T>` to write functions and types that work with any type:

```writ
func first<T>(items: Array<T>) -> Optional<T> {
    if items.isEmpty() { return null }
    return items[0]
}

let names: Array<string> = ["Alice", "Bob"]
let name = first(names)   // Optional<string>
```

### Built-in generic types

```writ
Array<string>
Dictionary<string, int>
Optional<Player>
Result<float>
Array<Dictionary<string, Player>>   // nesting works
```

---

## Type casting

Use `as` to convert between numeric types:

```writ
let n = 3.9 as int      // 3 (truncates)
let f = 42 as float     // 42.0
```
