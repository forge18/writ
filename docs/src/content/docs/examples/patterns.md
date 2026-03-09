---
title: Patterns
description: Common idioms, recipes, and best practices in Writ.
---

## State machine with enums

```writ
enum GameState {
    Menu, Playing, Paused, GameOver
}

var state = GameState.Menu

func handleInput(input: string) {
    when state {
        is GameState.Menu => {
            if input == "start" { state = GameState.Playing }
        }
        is GameState.Playing => {
            when input {
                "pause" => state = GameState.Paused
                "quit"  => state = GameState.GameOver
            }
        }
        is GameState.Paused => {
            if input == "resume" { state = GameState.Playing }
        }
        is GameState.GameOver => {
            if input == "restart" { state = GameState.Menu }
        }
    }
}
```

---

## Error propagation chain

```writ
func loadConfig(path: string) -> Result<Dictionary<string, string>> {
    let content = readFile(path)?
    let parsed = parseConfig(content)?
    return Success(parsed)
}

func initialize() -> Result<bool> {
    let config = loadConfig("game.cfg")?
    let level = config["startLevel"] ?? "1"
    loadLevel(level)?
    return Success(true)
}

// At the top level, handle the final result
when initialize() {
    is Success(ok) => print("Game initialized")
    is Error(msg)  => print("Failed: " .. msg)
}
```

---

## Collection pipeline

```writ
let players = getActivePlayers()

let leaderboard = players
    .filter((p: Player) => p.score > 0)
    .sort((a: Player, b: Player) => b.score - a.score)
    .map((p: Player) => p.name .. ": " .. p.score)

for entry in leaderboard {
    print(entry)
}
```

---

## Event callback with lambdas

```writ
class Button {
    public label: string
    private onClick: (string) => void

    public func setOnClick(handler: (string) => void) {
        onClick = handler
    }

    public func press() {
        onClick(label)
    }
}

let btn = Button(label: "Start")
btn.setOnClick((label: string) => {
    print(label .. " was pressed!")
})
btn.press()  // "Start was pressed!"
```

---

## Optional chaining

```writ
// Deep optional access with safe navigation
let weaponName = player?.inventory?.equippedWeapon?.name ?? "unarmed"

// Conditional method call
player?.inventory?.sort()

// Combined with Result
let damage = calculateDamage(attacker, target) ?? 0.0
```

---

## Coroutine sequences

```writ
func spawnWave(enemies: Array<string>) {
    for name in enemies {
        yield spawn(name)
        yield waitForSeconds(0.5)
    }
    print("Wave complete!")
}

func gameLoop() {
    yield spawnWave(["goblin", "goblin", "orc"])
    yield waitForSeconds(3.0)
    yield spawnWave(["orc", "orc", "troll"])
    yield waitForSeconds(3.0)
    yield spawnWave(["dragon"])
    print("All waves complete!")
}

start gameLoop()
```

---

## Builder pattern with named args

```writ
class Config {
    public width: int = 800
    public height: int = 600
    public title: string = "Game"
    public fullscreen: bool = false
    public vsync: bool = true
}

// Named args act like a builder — set only what you need
let cfg = Config(
    title: "My Game",
    width: 1920,
    height: 1080,
    fullscreen: true
)
```

---

## Trait-based polymorphism

```writ
trait Serializable {
    func serialize() -> string
}

class Player with Serializable {
    public name: string
    public score: int

    func serialize() -> string {
        return "$name:$score"
    }
}

class Enemy with Serializable {
    public type: string
    public health: float

    func serialize() -> string {
        return "$type:$health"
    }
}

func saveAll(items: Array<Serializable>) {
    var output = ""
    for item in items {
        output = output .. item.serialize() .. "\n"
    }
    writeFile("save.dat", output)
}
```
