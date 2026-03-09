---
title: Data
description: String, array, and dictionary methods.
---

## String

Module name: `"string"`. Called as methods on string values.

| Method       | Signature                                    | Description                |
|--------------|----------------------------------------------|----------------------------|
| `len`        | `() -> int`                                  | Character count            |
| `trim`       | `() -> string`                               | Strip leading/trailing whitespace |
| `trimStart`  | `() -> string`                               | Strip leading whitespace   |
| `trimEnd`    | `() -> string`                               | Strip trailing whitespace  |
| `toUpper`    | `() -> string`                               | Uppercase copy             |
| `toLower`    | `() -> string`                               | Lowercase copy             |
| `contains`   | `(sub: string) -> bool`                      | Substring check            |
| `startsWith` | `(prefix: string) -> bool`                   | Prefix check               |
| `endsWith`   | `(suffix: string) -> bool`                   | Suffix check               |
| `replace`    | `(from: string, to: string) -> string`       | Replace first occurrence   |
| `split`      | `(sep: string) -> Array<string>`             | Split into parts           |
| `join`       | `(sep: string) -> string`                    | Join array with separator  |
| `charAt`     | `(index: int) -> string`                     | Character at index         |
| `indexOf`    | `(sub: string) -> int`                       | Index of substring         |
| `parse`      | `() -> any`                                  | Parse to int or float      |

```writ
let s = "  Hello, World!  "

s.len()                    // 18
s.trim()                   // "Hello, World!"
s.toUpper()                // "  HELLO, WORLD!  "

s.contains("World")        // true
s.replace("World", "Writ") // "  Hello, Writ!  "
s.split(", ")              // ["  Hello", "World!  "]
["a", "b", "c"].join(", ") // "a, b, c"

"42".parse()               // 42 (int)
"3.14".parse()             // 3.14 (float)
```

---

## Array

Module name: `"array"`. Called as methods on `Array<T>` values.

| Method     | Signature                                          | Description                   |
|------------|----------------------------------------------------|-------------------------------|
| `len`      | `() -> int`                                        | Element count                 |
| `isEmpty`  | `() -> bool`                                       | True if empty                 |
| `contains` | `(value: T) -> bool`                               | Element check                 |
| `indexOf`  | `(value: T) -> int`                                | Index of element              |
| `push`     | `(value: T)`                                       | Append to end                 |
| `pop`      | `() -> T`                                          | Remove and return last        |
| `insert`   | `(index: int, value: T)`                           | Insert at index               |
| `remove`   | `(index: int)`                                     | Remove at index               |
| `first`    | `() -> Optional<T>`                                | First element                 |
| `last`     | `() -> Optional<T>`                                | Last element                  |
| `slice`    | `(start: int, end: int) -> Array<T>`               | Subarray (exclusive end)      |
| `reverse`  | `()`                                               | Reverse in place              |
| `sort`     | `()`                                               | Sort in place                 |
| `map`      | `(fn: (T) -> U) -> Array<U>`                       | Transform each element        |
| `filter`   | `(fn: (T) -> bool) -> Array<T>`                    | Keep matching elements        |
| `reduce`   | `(init: U, fn: (U, T) -> U) -> U`                 | Fold into a single value      |

```writ
let items = ["sword", "shield", "potion"]

items.len()               // 3
items.contains("sword")   // true
items.indexOf("shield")   // 1

items.push("dagger")
items.pop()               // "dagger"
items.first()             // Optional<T> — first element
items.slice(1, 3)         // subarray from index 1 to 3 (exclusive)

// Higher order
items.map((x: string) => x.toUpper())
items.filter((x: string) => x.len() > 4)
items.reduce(0, (acc: int, x: string) => acc + x.len())
```

---

## Dictionary

Module name: `"dictionary"`. Called as methods on `Dictionary<K, V>` values.

| Method   | Signature                                        | Description               |
|----------|--------------------------------------------------|---------------------------|
| `len`    | `() -> int`                                      | Entry count               |
| `isEmpty`| `() -> bool`                                     | True if empty             |
| `has`    | `(key: K) -> bool`                               | Key check                 |
| `keys`   | `() -> Array<K>`                                 | All keys                  |
| `values` | `() -> Array<V>`                                 | All values                |
| `remove` | `(key: K)`                                       | Remove by key             |
| `merge`  | `(other: Dictionary<K, V>) -> Dictionary<K, V>`  | Returns new merged dict   |

```writ
let scores = {"alice": 100, "bob": 95}

scores.len()              // 2
scores.has("alice")       // true
scores.keys()             // Array<string>
scores.values()           // Array<int>

scores.remove("bob")
scores.merge(otherDict)   // returns new merged dict
```
