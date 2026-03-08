---
title: Writ Language Specification
---

# Writ Language Specification

> Work in progress — decisions made collaboratively, subject to revision.

---

## Overview

Writ is a statically typed, embedded scripting language designed for games and applications. It prioritizes low cognitive complexity and approachability while being powerful enough to interface cleanly with Rust host applications.

**File extension:** `.writ`

**Design philosophy:**

- GDScript's approachability and game-dev ergonomics
- C#-inspired syntax, cleaned up and modernized
- Low cognitive complexity — easy to learn, scales to powerful usage
- Clean Rust interop — near-zero marshalling cost
- Small surface area — no legacy baggage

**Primary use case:** Embedded scripting inside games and applications.

**Paradigm:** Object-oriented core with functional features.

---

## Primitive Types

Primitives are always lowercase.

| Type       | Rust equivalent | Description                 |
|------------|-----------------|-----------------------------|
| `int`      | `i32`           | Whole number                |
| `bigint`   | `i64`           | Large whole number          |
| `float`    | `f32`           | Decimal number              |
| `bigfloat` | `f64`           | High precision decimal      |
| `bool`     | `bool`          | True/false                  |
| `string`   | `String`        | Text                        |

---

## Naming Conventions

| Kind                  | Convention | Example                      |
|-----------------------|------------|------------------------------|
| Primitives            | lowercase  | `int`, `float`, `string`     |
| User-defined types    | PascalCase | `Player`, `Entity`, `Weapon` |
| Methods and functions | camelCase  | `takeDamage`, `getHealth`    |
| Fields                | camelCase  | `maxHealth`, `currentSpeed`  |
| Enum variants         | PascalCase | `Direction.North`            |

---

## Visibility

Private by default. `public` to expose, `private` optional for explicitness.

```writ
class Player {
    public name: string          // explicitly public
    public health: float = 100.0 // explicitly public
    private speed: float = 5.0  // explicitly private
    weapons: Array<Weapon> = [] // implicitly private
}
```

---

## Variable Declaration

Block-based scoping. Type can be inferred from literal when unambiguous.

| Keyword | Meaning                              |
|---------|--------------------------------------|
| `const` | Compile-time constant, never changes |
| `let`   | Runtime immutable, set once          |
| `var`   | Mutable, can be reassigned           |

```writ
const MAX_HEALTH = 100.0
let name: string = "Hero"
let name = "Hero"            // inferred
var health: float = 100.0
var health = 100.0           // inferred
```

---

## Null Safety

Types are non-nullable by default. Use `Optional<T>` to allow absence of a value.

```writ
let name: string = "Hero"          // never null
let nickname: Optional<string>     // may have no value

// Check for value
if nickname.hasValue {
    print(nickname)                // compiler knows it's safe here
}

// Null coalescing
let n = nickname ?? "Unknown"

// Safe member access
let length = nickname?.length
```

---

## Error Handling

Functions that can fail return `Result<T>`. Errors are always string messages.

```writ
func divide(a: float, b: float) -> Result<float> {
    if b == 0 {
        return Error("Division by zero")
    }
    return Success(a / b)
}

// Pattern match on result
when result {
    is Success(value) => print(value)
    is Error(msg) => print(msg)
}

// Propagate error up the call stack
let value = divide(10, 0)?

// Fallback value
let value = divide(10, 0) ?? 0.0
```

---

## Functions

```writ
// Basic function
func takeDamage(amount: float) {
    health -= amount
}

// With return type
func divide(a: float, b: float) -> Result<float> {
    return Success(a / b)
}

// Void return - both are valid
func doSomething() { }
func doSomething() -> void { }

// Static method
static func create(name: string) -> Player {
    return Player(name: name)
}

// Named and positional parameters both valid
let p1 = Player(name: "Hero", health: 80.0)
let p2 = Player("Hero", 80.0)

// Variadic
func sum(...numbers: int) -> int {
    var total = 0
    for n in numbers {
        total += n
    }
    return total
}
```

### Lambdas

Parameter types required, return type always inferred.

```writ
// Inline single expression
let double = (x: float) => x * 2

// Inline multi-line
let onDamage = (amount: float) => {
    print("Took " .. amount .. " damage")
}

// Mutable lambda
var handler = (amount: float) => {
    print(amount)
}
```

`return` inside a lambda returns from the lambda only, never the enclosing function.

---

## Classes

Single inheritance via `extends`, multiple traits via `with`.

```writ
class Player extends Entity with Damageable, Updatable {
    public name: string
    public health: float = 100.0
        set(value) { field = clamp(value, 0.0, 100.0) }

    private speed: float = 5.0
    weapons: Array<Weapon> = []

    // Constructors are auto-generated from fields
    // Custom constructor only when special logic needed

    public static func create(name: string) -> Player {
        return Player(name: name)
    }

    public func takeDamage(amount: float) {
        health -= amount
        if health <= 0 {
            die()
        }
    }

    func updateSpeed() {   // implicitly private
        speed = calculateSpeed()
    }
}
```

### Constructors

Default and memberwise constructors are auto-generated from field declarations. Write a custom constructor only when special logic is needed.

```writ
// Both work automatically
let p1 = Player()
let p2 = Player(name: "Hero", health: 80.0)
```

### Getters and Setters

Kotlin-style — declare field normally, add accessor logic only when needed.

```writ
public health: float = 100.0
    set(value) { field = clamp(value, 0.0, maxHealth) }
```

### self

`self` is available implicitly inside instance methods.

```writ
func takeDamage(amount: float) {
    self.health -= amount  // self optional but available
    health -= amount       // also valid
}
```

---

## Traits

Replace both interfaces and abstract classes. Can have default implementations.

```writ
trait Damageable {
    // No default - must be implemented
    func takeDamage(amount: float)

    // Default implementation - can be overridden
    func die() {
        print("Entity died")
    }
}

trait Updatable {
    func update(delta: float)
}

class Player extends Entity with Damageable, Updatable {
    func takeDamage(amount: float) {
        health -= amount
    }

    func update(delta: float) {
        // implementation
    }

    // die() inherited from Damageable default
}
```

---

## Enums

Java-style — can have fields and methods.

```writ
// Simple enum
enum Direction {
    North, South, East, West
}

// With fields and methods
enum Status {
    Alive(100), Dead(0), Wounded(50)

    health: int

    func getHealth() -> int {
        return health
    }
}

// Usage
let dir = Direction.North

when dir {
    is Direction.North => moveNorth()
    is Direction.South => moveSouth()
    is Direction.East => moveEast()
    is Direction.West => moveWest()
}
```

---

## Collections

| Type               | Description                    |
|--------------------|--------------------------------|
| `Array<T>`         | Ordered, typed, indexed by int |
| `Dictionary<K, V>` | Key-value, typed, dot access   |

Generics compose freely.

```writ
// Arrays
let items: Array<string> = ["sword", "shield"]
let items = ["sword", "shield"]              // inferred
let teams: Array<Dictionary<string, Player>>
let grid: Array<Array<int>>

// Dictionaries
let scores: Dictionary<string, int> = {"alice": 100, "bob": 95}
let nested: Dictionary<string, Dictionary<string, int>>

// Spread
let combined = [...array1, ...array2]
let merged = {...dict1, ...dict2}
```

Traits can be used as Dictionary value types for heterogeneous structured data:

```writ
trait EntityConfig {
    func getName() -> string
    func getHealth() -> int
    func getSpeed() -> float
}

let configs: Dictionary<string, EntityConfig>
```

---

## Control Flow

### if / else

```writ
if health <= 0 {
    die()
} else if health < 25 {
    playLowHealthSound()
} else {
    heal()
}
```

### Ternary

```writ
let status = health > 0 ? "alive" : "dead"
```

### when

Kotlin-style pattern matching. Two forms — with and without a subject. Exhaustive — compiler warns if not all cases are handled.

```
// Value matching
when health {
    0 => print("Dead")
    100 => print("Full health")
    else => print("Damaged")
}

// Multiple values per arm
when health {
    0, 1, 2 => print("Critical")
    else => print("OK")
}

// Range matching
when health {
    0..25 => print("Critical")
    26..=100 => print("OK")
}

// Type matching with binding
when result {
    is Success(value) => print(value)
    is Error(msg) => print(msg)
}

// Guard clauses
when health {
    x if x < 0 => print("Invalid")
    x if x < 25 => print("Critical: $x")
    else => print("OK")
}

// Multi-line arms
when result {
    is Success(value) => {
        print(value)
        log(value)
    }
    is Error(msg) => print(msg)
}

// Without subject - replaces if/else chains
when {
    health == 100 => print("Full health")
    health <= 0 => print("Dead")
    else => print("Damaged")
}
```

### Loops

```writ
// While
while health > 0 {
    update()
}

// Collection iteration
for item in items {
    print(item)
}

// Range - exclusive
for i in 0..10 {
    print(i)  // 0 to 9
}

// Range - inclusive
for i in 0..=10 {
    print(i)  // 0 to 10
}

// Loop control
break     // exit loop
continue  // skip to next iteration
```

---

## Tuples

Rust-style with destructuring.

```writ
// Declaration
let point = (10.0, 20.0)
let player = ("Hero", 100.0, true)

// Typed
let point: (float, float) = (10.0, 20.0)

// Destructuring
let (x, y) = point
let (name, health, isActive) = player

// Function returning multiple values
func getPosition() -> (float, float) {
    return (x, y)
}

let (x, y) = getPosition()
```

---

## Strings

```writ
// Standard with interpolation
let greeting = "Hello $name"
let message = "Health: ${player.health}"
let calc = "Result: ${a + b}"

// Escape interpolation
let literal = "Hello \$name"  // prints: Hello $name

// Multi-line with interpolation
let message = """
    Player $name has $health health.
    Located at ${position.x}, ${position.y}.
    """

// Concatenation
let full = "Hello" .. " " .. name
```

---

## Operators

### Arithmetic

```
+ - * / %
```

### Comparison

```
== != < > <= >=
```

### Logical

```
&& || !
```

### Assignment

```
= += -= *= /= %=
```

### Other

| Operator | Meaning              |
|----------|----------------------|
| `??`     | Null coalescing      |
| `?.`     | Safe member access   |
| `?`      | Error propagation    |
| `..`     | String concatenation |
| `...`    | Spread               |
| `as`     | Type casting         |
| `? :`    | Ternary              |

### Ranges

| Syntax   | Meaning             |
|----------|---------------------|
| `0..10`  | Exclusive (0 to 9)  |
| `0..=10` | Inclusive (0 to 10) |

Operator overloading is handled entirely by the Rust binding layer. Not available in the scripting language itself.

---

## Type Casting

```writ
let x = someValue as float
let n = health as int
```

---

## Generics

Standard `<>` syntax. Compose freely.

```writ
Array<string>
Dictionary<string, int>
Optional<float>
Result<float>
Dictionary<string, Dictionary<string, Player>>
Array<Dictionary<string, int>>
```

---

## Comments

Go-style doc comments — plain `//` comments directly above a declaration. No special syntax required; tooling picks them up automatically.

```writ
// Calculates damage dealt to the player.
// Takes armor into account before applying to health.
// Returns a Result indicating if the player died.
func takeDamage(amount: float) -> Result<bool> {
    health -= amount
    return Success(health <= 0)
}

// Single line comment
/* Multi line comment */
```

---

## Blocks and Statements

- Curly braces `{}` delimit blocks
- Semicolons are optional — newlines terminate statements
- Unclosed brackets or parentheses defer termination to the closing bracket

```writ
// Both valid
let x = 10
let x = 10;

// Multi-line expression - parser waits for closing paren
let result = someFunction(
    argument1,
    argument2
)
```

---

## Execution Model

**Bytecode VM** — scripts are compiled to bytecode at load time and executed on a lightweight VM embedded in the Rust host.

**Pipeline:**

```
Source → Lexer → Parser → AST → Type Checker → Bytecode → VM
```

**Key properties:**

- Scripts loaded and executed at runtime — no ahead-of-time compilation required
- Static typing means all type checking happens before bytecode is emitted — no runtime type checks
- Near-zero marshalling cost — script types map directly to Rust types in memory
- VM starts with no external access — host explicitly registers what scripts can use

---

## Rust Interop

### Type Mapping

All primitive types map directly to Rust equivalents with identical memory layout — no conversion cost at the boundary.

User-defined types compile to first-class Rust types, not VM-managed wrappers.

### Inheritance Compilation

Single inheritance via `extends` compiles to **composition with `Deref`** in Rust. The compiler generates this automatically — neither the scripter nor the host developer sees the implementation.

```rust
// Generated from: class Player extends Entity { pub health: float }
pub struct Player {
    base: Entity,
    pub health: f32,
}

impl std::ops::Deref for Player {
    type Target = Entity;
    fn deref(&self) -> &Entity { &self.base }
}

impl std::ops::DerefMut for Player {
    fn deref_mut(&mut self) -> &mut Entity { &mut self.base }
}
```

### Binding Model

The VM starts with zero external access. The host explicitly opts in to what scripts can use:

```rust
let mut vm = Writ::new();
vm.register_type("Player", |args| { /* ... */ Ok(Box::new(player)) });
vm.register_host_fn("move", vec![Type::Int], Type::Void, fn1(player_move));
vm.register_host_fn("spawn", vec![Type::Str], Type::Void, fn1(entity_spawn));
```

Different VM instances can have different capabilities — mod scripts get a restricted API surface, core game scripts get full access. Same language, same VM, same execution model. The difference is purely what the host registers.

### Collections

| Script type | Rust type |
|---|---|
| `Array<T>` | `Vec<T>` |
| `Dictionary<K, V>` | `HashMap<K, V>` |
| `Optional<T>` | `Option<T>` |
| `Result<T>` | `Result<T, String>` |

### Traits

Script traits compile to Rust traits. Default implementations are preserved.

---

## Module System

TypeScript-style named exports and imports. Host/engine types are always globally available — no import needed.

### Exporting

```
// weapon.writ
export class Weapon {
    public damage: float = 10.0

    public func attack(target: Entity) {
        target.takeDamage(damage)
    }
}

export func createWeapon(damage: float) -> Weapon {
    return Weapon(damage: damage)
}
```

### Importing

```
// Named imports
import { Weapon, createWeapon } from "items/weapon"

// Wildcard import
import * as enemy from "entities/enemy"

class Player {
    pub weapon: Weapon

    func attack(target: enemy::Enemy) {
        weapon.attack(target)
    }
}
```

**Rules:**

- Named exports only — no default exports
- Host/engine types always globally available, never imported
- `::` for namespace access on wildcard imports
- Explicit imports make dependencies traceable

---

## Memory Management

Three-tier model — no garbage collector, no GC pauses.

**Stack allocation**

- Temporaries and local variables
- Automatic, zero overhead
- Freed when function returns

**Reference counting**

- Heap-allocated objects — collections, script-created instances
- Freed when no more references exist
- Predictable, no pause spikes
- Circular references handled via weak references

**Host ownership**

- Entity-bound script objects owned by Rust host
- Rust's ownership system manages lifetime
- When entity is destroyed, script goes with it
- Zero VM overhead for host-owned objects

---

## Coroutines

GDScript-style syntax with Kotlin-style structured concurrency. Any function containing `yield` is implicitly a coroutine — no special declaration needed. Coroutines are tied to the owning object's lifetime — when the object is destroyed, all its coroutines are automatically cancelled.

```
// Any function with yield is a coroutine
func openDoor() {
    playAnimation("door_open")
    yield waitForSeconds(2.0)
    setCollider(false)
}

// Start a coroutine
start openDoor()

// Yield variants
yield                           // wait one frame
yield waitForSeconds(5.0)       // wait N seconds
yield waitForFrames(10)         // wait N frames
yield waitUntil(() => isReady)  // wait for condition
yield openDoor()                // wait for another coroutine

// Return values
func getInput() -> string {
    yield waitForKeyPress()
    return lastKey
}

let key = yield getInput()
```

**Structured concurrency rules:**

- Coroutines are tied to the owning object's lifetime
- Object destroyed → all its coroutines cancelled automatically
- Cancellation propagates to child coroutines
- No dangling coroutines

```
class Door extends Entity {
    func interact() {
        start openDoor()  // tied to Door's lifetime
    }

    func openDoor() {
        yield waitForSeconds(2.0)
        setCollider(false)
    }
}
```

---

## Tooling

### Language Server Protocol (LSP)

Writ ships with an LSP server enabling editor integration. The LSP sits on top of the compiler pipeline — feeding source through the lexer, parser, and type checker and exposing results to the editor.

**Features:**
- Autocomplete
- Go to definition
- Find references
- Inline errors while typing
- Hover documentation
- Rename refactoring

### VS Code Extension

A VS Code extension ships alongside the LSP providing:
- Syntax highlighting
- Debugger integration via the VM's breakpoint API
- Hot reload trigger from the editor
- Error display inline

### Error Messages

Rust-style error messages — precise, plain English, actionable. The compiler always reports file, line, and column. Where possible it suggests a fix.

**Type mismatch**
```
Error: Type mismatch
  --> entities/player.writ:34:12
   |
34 |     health = "full"
   |              ^^^^^^ expected float, found string
```

**Unknown field with suggestion**
```
Error: Unknown field 'hp'
  --> entities/player.writ:34:12
   |
32 |     public func takeDamage(amount: float) {
33 |         self.hp -= amount
   |              ^^ no field named 'hp' on Player
34 |     }
   |
   = did you mean 'health'?
```

**Rules:**
- Always include file, line, and column
- Plain English — say what was expected and what was found
- Show surrounding lines for context
- Suggest fixes where the type checker can infer them
- Avoid cascading errors from a single mistake

---

## VM Features

### Stack Traces

All runtime errors include a full stack trace with file name, line number, and function name.

```
Error: Division by zero
  at divide (math/utils.writ:12)
  at calculateDamage (combat/damage.writ:34)
  at Player.takeDamage (entities/player.writ:67)
```

### Hot Reload

Scripts can be reloaded at runtime without restarting the host. Existing object state is preserved where possible. Designed for fast iteration during development.

```rust
vm.reload("entities/player.writ")
```

### Breakpoints

The host can set breakpoints on specific lines. Execution pauses when a breakpoint is hit, allowing inspection of the current state.

```rust
vm.set_breakpoint("entities/player.writ", 67)
vm.on_breakpoint(|ctx| {
    // inspect state
})
```

### Debug Hooks

The host can register callbacks that fire on every line, call, or return. Enables the host to build custom tooling — debuggers, profilers, loggers — on top of the VM.

```rust
vm.on_line(|file, line| { })
vm.on_call(|fn_name| { })
vm.on_return(|fn_name| { })
```

### Instruction Limit

The host can set a maximum instruction count per script execution. Protects against infinite loops in untrusted mod scripts. Configurable per VM instance.

```rust
vm.set_instruction_limit(1_000_000);  // kill after N instructions
```

When the limit is hit, the script is terminated and an error is returned to the host.

---

## Standard Library

Follows Lua's philosophy — small, complete, universal. The host provides domain-specific functionality. The standard library provides only what every script needs regardless of host.

Implemented as a thin scripting API layer over Rust's `std` wherever possible. External crates used only when `std` genuinely can't cover it.

### Basic

- `print` — output to host console
- `assert` — assert a condition, error if false
- `type` — get the type name of a value

### Math

Backed by `std::f32` / `std::f64`

- `abs`, `ceil`, `floor`, `round`, `sqrt`
- `sin`, `cos`, `tan`
- `min`, `max`, `clamp`
- `PI`, `TAU`, `INFINITY`
- `pow`, `log`, `exp`

### String

Backed by `std::string::String` / `std::str`

- `len`, `trim`, `trimStart`, `trimEnd`
- `toUpper`, `toLower`
- `contains`, `startsWith`, `endsWith`
- `replace`, `split`, `join`
- `charAt`, `indexOf`
- `parse` — convert string to other types

### Array

Backed by `std::vec::Vec`

- `push`, `pop`, `insert`, `remove`
- `len`, `isEmpty`
- `contains`, `indexOf`
- `reverse`, `sort`
- `map`, `filter`, `reduce`
- `first`, `last`
- `slice`

### Dictionary

Backed by `std::collections::HashMap`

- `keys`, `values`
- `has`, `remove`
- `len`, `isEmpty`
- `merge`

### I/O

Backed by `std::io` / `std::fs`. Can be disabled by host.

- `readFile`, `writeFile`
- `readLine`
- `fileExists`

### Time

Backed by `std::time`

- `now` — current timestamp
- `elapsed` — time since timestamp

### Random

Backed by `rand` crate — only external dependency in std lib.

- `random` — random float 0.0..1.0
- `randomInt(min, max)` — random int in range
- `randomFloat(min, max)` — random float in range
- `shuffle(array)` — shuffle array in place

---
