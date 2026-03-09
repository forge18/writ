---
title: Projects
description: Complete mini-programs demonstrating Writ in action.
---

## Game entity system

A combat system with players, enemies, and inventory.

```writ
trait Damageable {
    func takeDamage(amount: float)
    func isAlive() -> bool
}

struct Item {
    name: string
    damage: float = 0.0
    healing: float = 0.0
}

class Entity with Damageable {
    public name: string
    public health: float = 100.0
        set(value) { field = clamp(value, 0.0, maxHealth) }
    public maxHealth: float = 100.0

    func takeDamage(amount: float) {
        health -= amount
        print("$name takes $amount damage! HP: $health/$maxHealth")
    }

    func isAlive() -> bool {
        return health > 0
    }
}

class Player extends Entity {
    public inventory: Array<Item> = []

    public func addItem(item: Item) {
        inventory.push(item)
        print("$name picked up ${item.name}")
    }

    public func useItem(index: int) -> Result<bool> {
        if index < 0 || index >= inventory.len() {
            return Error("Invalid item index")
        }
        let item = inventory[index]
        if item.healing > 0 {
            health += item.healing
            print("$name heals for ${item.healing}! HP: $health/$maxHealth")
        }
        inventory.remove(index)
        return Success(true)
    }

    public func attack(target: Entity) {
        let weapon = inventory
            .filter((i: Item) => i.damage > 0)
            .first() ?? Item(name: "fists", damage: 5.0)
        print("$name attacks ${target.name} with ${weapon.name}!")
        target.takeDamage(weapon.damage)
    }
}

// Usage
let player = Player(name: "Hero", maxHealth: 120.0, health: 120.0)
let enemy = Entity(name: "Goblin", maxHealth: 50.0, health: 50.0)

player.addItem(Item(name: "Iron Sword", damage: 25.0))
player.addItem(Item(name: "Health Potion", healing: 40.0))

player.attack(enemy)
player.attack(enemy)

when enemy.isAlive() {
    true  => print("${enemy.name} is still standing!")
    false => print("${enemy.name} has been defeated!")
}
```

---

## State machine

A game state manager with transitions and frame updates.

```writ
enum State {
    MainMenu, Playing, Paused, GameOver
}

class GameManager {
    public state: State = State.MainMenu
    public score: int = 0
    public lives: int = 3

    public func transition(input: string) {
        when state {
            is State.MainMenu => {
                if input == "play" {
                    score = 0
                    lives = 3
                    state = State.Playing
                    print("Game started!")
                }
            }
            is State.Playing => {
                when input {
                    "pause" => {
                        state = State.Paused
                        print("Game paused")
                    }
                    "die" => {
                        lives -= 1
                        when {
                            lives <= 0 => {
                                state = State.GameOver
                                print("Game Over! Final score: $score")
                            }
                            else => print("Lives remaining: $lives")
                        }
                    }
                    "score" => {
                        score += 100
                        print("Score: $score")
                    }
                }
            }
            is State.Paused => {
                if input == "resume" {
                    state = State.Playing
                    print("Game resumed")
                }
            }
            is State.GameOver => {
                if input == "restart" {
                    state = State.MainMenu
                    print("Back to menu")
                }
            }
        }
    }
}

let game = GameManager()
game.transition("play")
game.transition("score")
game.transition("score")
game.transition("die")
game.transition("die")
game.transition("die")
game.transition("restart")
```

---

## Coroutine-driven dialogue system

A cutscene/dialogue system using coroutines for timing and sequencing.

```writ
class DialogueSystem {
    public isActive: bool = false

    public func showLine(speaker: string, text: string) {
        print("[$speaker]: $text")
        yield waitForSeconds(2.0)
    }

    public func showChoice(prompt: string, options: Array<string>) -> int {
        print(prompt)
        for i in 0..options.len() {
            print("  ${i + 1}. ${options[i]}")
        }
        yield waitForSeconds(1.0)
        return 0  // host would provide actual input
    }

    public func runIntro() {
        isActive = true

        yield showLine("Narrator", "The hero enters the dark cave...")
        yield waitForSeconds(1.0)

        yield showLine("Hero", "Hello? Is anyone there?")
        yield showLine("???", "Who dares disturb my slumber?")

        yield waitForSeconds(0.5)

        yield showLine("Dragon", "I am Valthor, guardian of the Crystal!")
        yield showLine("Hero", "I've come for the Crystal of Light.")

        let choice = yield showChoice("What do you do?", [
            "Fight the dragon",
            "Negotiate",
            "Run away"
        ])

        when choice {
            0 => yield fightScene()
            1 => yield negotiateScene()
            else => yield fleeScene()
        }

        isActive = false
    }

    func fightScene() {
        yield showLine("Hero", "Then I shall take it by force!")
        yield showLine("Dragon", "So be it, mortal!")
        yield waitForSeconds(1.0)
        print("[Combat begins]")
    }

    func negotiateScene() {
        yield showLine("Hero", "Perhaps we can make a deal?")
        yield showLine("Dragon", "Interesting... I'm listening.")
        yield waitForSeconds(1.0)
        print("[Negotiation begins]")
    }

    func fleeScene() {
        yield showLine("Hero", "On second thought...")
        print("[The hero runs away]")
    }
}

let dialogue = DialogueSystem()
start dialogue.runIntro()
```
