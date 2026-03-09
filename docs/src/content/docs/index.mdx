---
title: Writ
description: Writ is a statically typed scripting language designed for game developers. It embeds directly into Rust with near-zero interop cost — no marshalling, no runtime overhead, no fighting the borrow checker.
template: splash
hero:
  tagline: Native speed. Familiar syntax. Built for games.
  actions:
    - text: Get Started
      link: /writ/guides/getting-started/
      icon: right-arrow
    - text: Embedding Guide
      link: /writ/advanced/embedding/
      icon: document
      variant: minimal
    - text: GitHub
      link: https://github.com/forge18/writ
      icon: github
      variant: minimal
---

import { Card, CardGrid, Code } from '@astrojs/starlight/components';

## What Writ looks like

```writ
struct Vec2 {
    x: float
    y: float

    func length() -> float {
        return (x * x + y * y).sqrt()
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

## Embedding in four lines

```rust
use writ::Writ;

let mut vm = Writ::new();
vm.set_tick_source(|| engine.delta_time()); // coroutines just work
vm.load("scripts/game.writ").unwrap();
vm.call("onStart", &[]).unwrap();
```

<CardGrid>
  <Card title="Statically typed" icon="approve-check">
    All types checked before execution. No runtime surprises, no dynamic dispatch overhead.
  </Card>
  <Card title="Sandboxed by default" icon="shield">
    Scripts start with zero access. You register exactly what they can call — nothing more.
  </Card>
  <Card title="Hot reload" icon="rocket">
    Swap function bytecode at runtime without restarting. VM state is preserved across reloads.
  </Card>
  <Card title="Coroutines" icon="random">
    GDScript-style `yield` with structured lifetimes. Coroutines cancel automatically when their owner is destroyed.
  </Card>
  <Card title="Clean Rust interop" icon="puzzle">
    Implement `WritObject` on any Rust struct. No wrappers, no marshalling — your types map directly.
  </Card>
  <Card title="Full standard library" icon="list-format">
    Math, strings, arrays, dicts, regex, noise, tweening, timers — all disableable per instance.
  </Card>
</CardGrid>
