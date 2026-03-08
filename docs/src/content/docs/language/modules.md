---
title: Modules
description: Splitting scripts across files with import and export.
---

Scripts can be split across multiple files. Exports are explicit — nothing is public unless marked. Host-registered types are always globally available and never need importing.

## Exporting

Use `export` before any top-level declaration:

```writ
// weapons/sword.writ

export class Sword {
    public damage: float = 15.0
    public speed: float = 1.2

    public func attack(target: Entity) {
        target.takeDamage(damage)
    }
}

export func createSword(damage: float) -> Sword {
    return Sword(damage: damage)
}

// Not exported — internal to this file
func validateDamage(d: float) -> bool {
    return d > 0
}
```

## Importing

### Named imports

Import specific names from a file. Path is relative, no extension:

```writ
import { Sword, createSword } from "weapons/sword"

let s = createSword(damage: 20.0)
s.attack(enemy)
```

### Wildcard imports

Import everything under a namespace:

```writ
import * as weapons from "weapons/sword"

let s = weapons::createSword(damage: 20.0)
let blade: weapons::Sword = s
```

Use `::` for namespace access on wildcard imports.

## Rules

- **Named exports only** — no default exports
- **Host types are global** — `Player`, `World`, and anything the host registered are always available without importing
- **No circular imports** — files can't import each other
- **Explicit beats implicit** — if it's not exported, it doesn't exist outside the file

## Loading from the host

The host controls which files are loaded. Scripts don't load files themselves at runtime — the host calls `vm.load()`:

```rust
// Load a file — its exports become available to other scripts
vm.load("weapons/sword.writ").unwrap();
vm.load("entities/player.writ").unwrap();

// Now run a script that uses both
vm.run(r#"
    import { Sword } from "weapons/sword"
    import { Player } from "entities/player"
    // ...
"#).unwrap();
```
