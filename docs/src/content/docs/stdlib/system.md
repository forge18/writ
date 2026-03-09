---
title: System
description: I/O, time, random number generation, and regular expressions.
---

## I/O

Module name: `"io"`. Can be disabled for sandboxed scripts.

| Function     | Signature                                  | Description          |
|--------------|--------------------------------------------|----------------------|
| `readFile`   | `(path: string) -> string`                 | Read file contents   |
| `writeFile`  | `(path: string, content: string)`          | Write to file        |
| `readLine`   | `() -> string`                             | Read line from stdin |
| `fileExists` | `(path: string) -> bool`                   | Check if file exists |

```writ
let content = readFile("data/config.writ")
writeFile("output/log.txt", "Game started")
let line = readLine()
let exists = fileExists("saves/slot1.dat")
```

---

## Time

Module name: `"time"`.

| Function  | Signature                       | Description              |
|-----------|---------------------------------|--------------------------|
| `now`     | `() -> float`                   | Current timestamp (seconds) |
| `elapsed` | `(start: float) -> float`      | Seconds since start      |

```writ
let start = now()
// ... do work ...
let elapsed = elapsed(start)  // seconds since start
```

---

## Random

Module name: `"random"`. Backed by the `rand` crate.

| Function      | Signature                                    | Description              |
|---------------|----------------------------------------------|--------------------------|
| `random`      | `() -> float`                                | Float in 0.0..1.0        |
| `randomInt`   | `(min: int, max: int) -> int`                | Int in min..=max          |
| `randomFloat` | `(min: float, max: float) -> float`          | Float in range            |
| `shuffle`     | `(arr: Array<T>)`                            | Shuffle array in place    |

```writ
random()                   // float in 0.0..1.0
randomInt(1, 6)            // int in 1..=6 (inclusive)
randomFloat(0.0, 100.0)    // float in range
shuffle(myArray)           // shuffles array in place
```

---

## Regex

Module name: `"regex"`. Backed by the `regex` crate.

| Method       | Signature                                  | Description              |
|--------------|--------------------------------------------|--------------------------|
| `test`       | `(input: string) -> bool`                  | Does pattern match?      |
| `match`      | `(input: string) -> Optional<string>`      | First match              |
| `matchAll`   | `(input: string) -> Array<string>`         | All matches              |
| `replace`    | `(input: string, rep: string) -> string`   | Replace first occurrence |
| `replaceAll` | `(input: string, rep: string) -> string`   | Replace all occurrences  |

```writ
let re = Regex("\\d+")

re.test("abc123")          // true
re.match("abc123")         // Optional<string> — first match
re.matchAll("a1b2c3")      // Array<string> — all matches
re.replace("a1b2", "X")    // "aXb2"
re.replaceAll("a1b2", "X") // "aXbX"
```
