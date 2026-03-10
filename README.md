# Writ

[![CI](https://github.com/forge18/writ/actions/workflows/ci.yml/badge.svg)](https://github.com/forge18/writ/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/forge18/writ/branch/main/graph/badge.svg)](https://codecov.io/gh/forge18/writ)
[![alpha](https://img.shields.io/badge/status-alpha-orange)](https://github.com/forge18/writ)
[![license](https://img.shields.io/badge/license-MIT-blue)](LICENSE)

> **⚠ Alpha:** Writ is not production-ready. APIs will change, features are incomplete, and there are known bugs. Use at your own risk.

Native speed. Familiar syntax. Scripting for games.

Writ is a statically typed scripting language designed for game developers. It embeds directly into Rust with near-zero interop cost — no marshalling, no runtime overhead, no fighting the borrow checker. Familiar to anyone who has written GDScript or C#. Fast enough to run in a real-time game loop. Write game logic in Writ. Let Rust handle the rest.

---

## What it looks like

```writ
struct Vec2 {
    x: float
    y: float

    func length() -> float {
        return sqrt(x * x + y * y)
    }
}

class Player extends Entity {
    public health: float = 100.0
        set(value) { field = clamp(value, 0.0, 100.0) }

    public func takeDamage(amount: float) {
        health -= amount
        if health <= 0 { die() }
    }

    func respawn() {
        yield seconds(3.0)   // wait without blocking the host
        health = 100.0
        setActive(true)
    }
}
```

---

## Embedding

```rust
use writ::Writ;

let mut vm = Writ::new();                           // create a sandboxed VM
vm.set_tick_source(|| engine.delta_time());         // drive coroutine timers from your engine
vm.load("scripts/game.writ").unwrap();              // compile and load a script
vm.call("onStart", &[]).unwrap();                   // call any top-level function by name
```

---

## Features

- **Statically typed** — catch mistakes at compile time, not during a play session
- **Clean Rust interop** — derive `WritObject` on any Rust struct; no wrappers, no copying
- **Sandboxed by default** — scripts start with zero access; register exactly what they can call
- **Hot reload** — swap bytecode mid-session without restarting; VM state and coroutines survive
- **Coroutines** — `yield` suspends a function and resumes it next tick; no threads, no callbacks
- **Standard library** — math, strings, collections, regex, noise, tweening, timers; each module toggleable per VM

---

## Installation

```toml
[dependencies]
writ = { git = "https://github.com/forge18/writ" }
```

---

## Documentation

**[forge18.github.io/writ](https://forge18.github.io/writ)** — language reference, embedding guide, examples, and stdlib docs.

---

## License

MIT
