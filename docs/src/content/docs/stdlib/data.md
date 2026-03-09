---
title: Data
description: String, array, and dictionary methods.
---

## String

Module name: `"string"`. Called as methods on string values.

```writ
let s = "  Hello, World!  "

s.len()                    // 18
s.trim()                   // "Hello, World!"
s.trimStart()              // "Hello, World!  "
s.trimEnd()                // "  Hello, World!"
s.toUpper()                // "  HELLO, WORLD!  "
s.toLower()                // "  hello, world!  "

s.contains("World")        // true
s.startsWith("  Hello")    // true
s.endsWith("  ")           // true

s.replace("World", "Writ") // "  Hello, Writ!  "
s.split(", ")              // ["  Hello", "World!  "]
["a", "b", "c"].join(", ") // "a, b, c"

s.charAt(2)                // "H"
s.indexOf("World")         // 9

"42".parse()               // 42 (int)
"3.14".parse()             // 3.14 (float)
```

---

## Array

Module name: `"array"`. Called as methods on `Array<T>` values.

```writ
let items = ["sword", "shield", "potion"]

items.len()               // 3
items.isEmpty()           // false
items.contains("sword")   // true
items.indexOf("shield")   // 1

items.push("dagger")
items.pop()               // "dagger"
items.insert(1, "axe")
items.remove(1)

items.first()             // Optional<T> — first element
items.last()              // Optional<T> — last element
items.slice(1, 3)         // subarray from index 1 to 3 (exclusive)

items.reverse()
items.sort()

// Higher order
items.map((x: string) => x.toUpper())
items.filter((x: string) => x.len() > 4)
items.reduce(0, (acc: int, x: string) => acc + x.len())
```

---

## Dictionary

Module name: `"dictionary"`. Called as methods on `Dictionary<K, V>` values.

```writ
let scores = {"alice": 100, "bob": 95}

scores.len()              // 2
scores.isEmpty()          // false
scores.has("alice")       // true

scores.keys()             // Array<string>
scores.values()           // Array<int>

scores.remove("bob")
scores.merge(otherDict)   // returns new merged dict
```
