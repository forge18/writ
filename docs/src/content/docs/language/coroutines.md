---
title: Coroutines
description: Async-style scripting with yield and structured lifetime management.
---

Coroutines let scripts do work across multiple frames without blocking the host. Any function that contains `yield` is implicitly a coroutine — no special declaration or wrapping needed.

The host application sets up a tick source once during initialization — after that, coroutines advance automatically. If you're embedding Writ yourself, see [Runtime & Memory](/writ/advanced/runtime/) for the one-time setup.

## Starting a coroutine

Use `start` to launch a coroutine in the background. Execution returns to the caller immediately:

```writ
func openDoor() {
    playAnimation("door_open")
    yield waitForSeconds(2.0)    // wait 2 seconds, then continue
    setCollider(false)
    playAnimation("door_idle")
}

start openDoor()    // non-blocking
doSomethingElse()   // runs right away
```

## Yield variants

```writ
yield                                    // wait one frame
yield waitForSeconds(2.5)                // wait N seconds
yield waitForFrames(10)                  // wait N frames
yield waitUntil(() => enemy == null)     // wait for a condition

yield otherCoroutine()           // wait for another coroutine to finish
```

## Waiting for a coroutine

`yield` on a coroutine call suspends the current coroutine until the child finishes:

```writ
func spawnAndWait() {
    yield spawnEnemy()     // wait for spawnEnemy to complete
    print("Spawn done")
}

func spawnEnemy() {
    playAnimation("spawn")
    yield waitForSeconds(1.0)
    setActive(true)
}
```

## Return values

Coroutines can return values. Use `yield` at the call site to receive them:

```writ
func getInput() -> string {
    yield waitForKeyPress()
    return lastKey
}

let key = yield getInput()
print("Pressed: " .. key)
```

## Structured lifetime

Coroutines are tied to their owning object. When the object is destroyed, all its coroutines are automatically cancelled — including any child coroutines they started.

```writ
class Door extends Entity {
    func interact() {
        start openSequence()   // tied to this Door
    }

    func openSequence() {
        yield waitForSeconds(0.5)
        playSound("creak")
        yield waitForSeconds(1.5)
        setCollider(false)
    }
}
```

If the `Door` entity is destroyed while `openSequence` is waiting, the coroutine is cancelled cleanly. No callbacks, no manual cleanup.

## Common patterns

### Countdown

```writ
func countdown(from: int) {
    var n = from
    while n > 0 {
        print(n)
        yield waitForSeconds(1.0)
        n -= 1
    }
    print("Go!")
}

start countdown(from: 3)
```

### Delayed action

```writ
func delayedHeal(amount: float, after: float) {
    yield waitForSeconds(after)
    health += amount
}

start delayedHeal(amount: 20.0, after: 3.0)
```

### Wait for condition

```writ
func waitForDeath() {
    yield waitUntil(() => health <= 0)
    playDeathAnimation()
    yield waitForSeconds(2.0)
    despawn()
}

start waitForDeath()
```

### Sequential steps

```writ
func bossIntro() {
    yield fadeIn()
    yield waitForSeconds(1.0)
    yield roar()
    yield waitForSeconds(0.5)
    setPhase(1)
}
```
