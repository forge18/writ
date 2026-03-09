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

Use `::` for namespace access on wildcard imports — valid in **both expression and type positions**:

```writ
import * as weapons from "weapons/sword"

// Expression position
let s = weapons::createSword(damage: 20.0)

// Type annotation position
let blade: weapons::Sword = s

func equip(item: weapons::Sword) -> weapons::Sword { ... }
```

## Rules

- **Named exports only** — no default exports
- **Host types are global** — `Player`, `World`, and anything the host registered are always available without importing
- **No circular imports** — files can't import each other
- **Explicit beats implicit** — if it's not exported, it doesn't exist outside the file
- **Automatic resolution** — import paths resolve relative to the importing file's directory, with `.writ` appended automatically
