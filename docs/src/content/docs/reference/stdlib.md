---
title: Standard Library
description: All built-in functions and types available to scripts.
---

The standard library is loaded automatically by `Writ::new()`. The host can disable individual modules via `vm.disable_module("name")`.

---

## Basic

Global functions always available.

| Function     | Signature                                        | Description                 |
|--------------|--------------------------------------------------|-----------------------------|
| `print`      | `(value: any)`                                   | Print to the host console   |
| `assert`     | `(condition: bool, msg: string)`                 | Error if condition is false |
| `typeof`     | `(value: any) -> string`                         | Type name of a value        |
| `instanceof` | `(value: any, type: string) -> bool`             | Check type at runtime       |
| `hasField`   | `(value: any, field: string) -> bool`            | Check if a field exists     |
| `getField`   | `(value: any, field: string) -> any`             | Get a field by name         |
| `fields`     | `(value: any) -> Array<string>`                  | All field names             |
| `methods`    | `(value: any) -> Array<string>`                  | All method names            |
| `hasMethod`  | `(value: any, method: string) -> bool`           | Check if a method exists    |
| `invoke`     | `(value: any, method: string, args: any) -> any` | Call a method by name       |

---

## Math

Module name: `"math"`

```writ
abs(-5.0)         // 5.0
ceil(1.2)         // 2.0
floor(1.9)        // 1.0
round(1.5)        // 2.0
sqrt(25.0)        // 5.0
pow(2.0, 10.0)    // 1024.0
log(100.0)        // 4.605...
exp(1.0)          // 2.718...

sin(PI / 2)       // 1.0
cos(0.0)          // 1.0
tan(PI / 4)       // 1.0

min(3.0, 5.0)     // 3.0
max(3.0, 5.0)     // 5.0
clamp(150.0, 0.0, 100.0)  // 100.0

// Constants
PI        // 3.14159...
TAU       // 6.28318...
INFINITY
```

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
scores.contains("alice")  // true

scores.keys()             // Array<string>
scores.values()           // Array<int>

scores.remove("bob")
scores.merge(otherDict)   // returns new merged dict
```

---

## I/O

Module name: `"io"`. Can be disabled for sandboxed scripts.

```writ
let content = readFile("data/config.writ")
writeFile("output/log.txt", "Game started")
let line = readLine()
let exists = fileExists("saves/slot1.dat")
```

---

## Time

Module name: `"time"`.

```writ
let start = now()          // current timestamp (float seconds)
// ... do work ...
let elapsed = elapsed(start)  // seconds since start
```

---

## Random

Module name: `"random"`. Backed by the `rand` crate.

```writ
random()                   // float in 0.0..1.0
randomInt(1, 6)            // int in 1..=6 (inclusive)
randomFloat(0.0, 100.0)    // float in range
shuffle(myArray)           // shuffles array in place
```

---

## Regex

Module name: `"regex"`. Backed by the `regex` crate.

```writ
let re = Regex("\\d+")

re.test("abc123")          // true — does pattern match?
re.match("abc123")         // Optional<string> — first match
re.matchAll("a1b2c3")      // Array<string> — all matches
re.replace("a1b2", "X")    // "aXb2" — replace first
re.replaceAll("a1b2", "X") // "aXbX" — replace all
```

---

## Math types

### Vec2 / Vec3 / Vec4

2D, 3D, and 4D vectors. Backed by `glam`.

```writ
let v2 = Vec2(x: 1.0, y: 0.0)
let v3 = Vec3(x: 0.0, y: 1.0, z: 0.0)

v2.length()
v2.normalized()
v2.dot(other)
v2.lerp(other, 0.5)

v3.cross(other)
v3.length()
v3.normalized()
```

### Mat3 / Mat4

3×3 and 4×4 matrices. Backed by `glam`.

```writ
let m = Mat4.identity()
let rot = Mat4.fromRotation(axis, angle)
let scale = Mat4.fromScale(sx, sy, sz)
let trans = Mat4.fromTranslation(tx, ty, tz)

m.multiply(other)
m.inverse()
m.transpose()
```

### Quaternion

Rotation representation. Backed by `glam`.

```writ
let q = Quaternion.fromAxisAngle(axis, angle)
let q = Quaternion.fromEuler(x, y, z)
let q = Quaternion.lookRotation(forward, up)

q.normalize()
q.slerp(other, t)
q.toMat4()
```

### Color

RGBA color.

```writ
let red = Color(r: 1.0, g: 0.0, b: 0.0, a: 1.0)
let c = Color.fromHex("#FF5733")
let c = Color.fromHSV(h, s, v)

c.lerp(other, 0.5)
```

### Rect / BoundingBox

2D rectangle and 3D bounding box.

```writ
let r = Rect(x: 0.0, y: 0.0, width: 100.0, height: 50.0)
r.contains(point)
r.intersects(other)
r.area()
```

---

## Interpolation

Module name: `"interpolation"`.

```writ
lerp(0.0, 100.0, 0.5)         // 50.0
smoothstep(0.0, 1.0, t)
inverseLerp(0.0, 100.0, 50.0) // 0.5
```

---

## Noise

Module name: `"noise"`. Backed by `fastnoise-lite`.

```writ
noise2D(x, y)              // float in -1.0..1.0
noise3D(x, y, z)
```

---

## Tween

Module name: `"tween"`. Animate a value over time.

```writ
let t = Tween(from: 0.0, to: 100.0, duration: 2.0)
t.update(delta)
let value = t.value()
t.isDone()
```

---

## Timer

Module name: `"timer"`.

```writ
let t = Timer(duration: 5.0)
t.update(delta)
t.isDone()
t.reset()
t.elapsed()
```
