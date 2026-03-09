---
title: Error Handling
description: Optionals, results, error propagation, and null safety.
---

Types are non-nullable by default in Writ. Use `Optional<T>` for values that may be absent, and `Result<T>` for operations that can fail.

## Optionals

Wrap in `Optional<T>` to allow absence of a value:

```writ
let target: Optional<Player>   // may be absent
```

### Check before using

```writ
if target.hasValue {
    target.takeDamage(10.0)
}
```

### Safe member access

Short-circuits to null if absent:

```writ
let hp = target?.health
```

### Null coalescing

Use a fallback if absent:

```writ
let hp = target?.health ?? 0.0
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

### Pattern matching

Handle results with `when`:

```writ
when divide(10.0, 2.0) {
    is Success(value) => print(value)
    is Error(msg)     => print("Failed: " .. msg)
}
```

### Error propagation

Propagate errors up with `?` — returns early on failure:

```writ
func calculate(a: float, b: float) -> Result<float> {
    let quotient = divide(a, b)?   // returns early on error
    return Success(quotient * 2.0)
}
```

### Fallback values

Use `??` instead of propagating:

```writ
let result = divide(10.0, 0.0) ?? 0.0
```
