---
title: System
description: I/O, time, random number generation, and regular expressions.
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
