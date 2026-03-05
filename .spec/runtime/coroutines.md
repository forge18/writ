# Coroutines

> **Crate:** `writ-vm` | **Status:** Draft

## 1. Purpose

This spec defines Writ's coroutine system — implicit declaration, yield variants, structured concurrency, and lifetime management. Coroutines are the primary mechanism for time-based scripting logic in games.

## 2. Dependencies

| Depends On                         | Relationship                            |
|------------------------------------|-----------------------------------------|
| [vm.md](vm.md)                     | Coroutine scheduler lives inside the VM |
| [syntax.md](../language/syntax.md) | `yield`, `start` keywords               |

---

## 3. Implicit Declaration

Any function containing `yield` is implicitly a coroutine. No special declaration or keyword is needed.

```writ
func openDoor() {         // implicitly a coroutine
    playAnimation("door_open")
    yield waitForSeconds(2.0)
    setCollider(false)
}

func update(delta: float) {   // regular function — no yield
    position.x += speed * delta
}
```

The type checker detects `yield` usage and marks the function as a coroutine automatically.

---

## 4. Starting Coroutines

Use `start` to launch a coroutine. The call returns immediately; the coroutine runs alongside other coroutines on subsequent frames.

```writ
start openDoor()
start patrolPath(waypoints)
```

---

## 5. Yield Variants

| Syntax                             | Behavior                                     |
|------------------------------------|----------------------------------------------|
| `yield`                            | Suspend for one frame                        |
| `yield waitForSeconds(n)`          | Suspend for N seconds                        |
| `yield waitForFrames(n)`           | Suspend for N frames                         |
| `yield waitUntil(() => condition)` | Suspend until condition returns true         |
| `yield anotherCoroutine()`         | Suspend until the called coroutine completes |

---

## 6. Return Values

Coroutines can return values. The caller suspends until the coroutine completes, then receives the return value.

```writ
func getInput() -> string {
    yield waitForKeyPress()
    return lastKey
}

let key = yield getInput()
```

---

## 7. Structured Concurrency

Writ uses Kotlin-style structured concurrency. Coroutines are tied to the lifetime of their owning object. When the object is destroyed, all its running coroutines are automatically cancelled.

```writ
class Door extends Entity {
    func interact() {
        start openDoor()   // tied to this Door instance
    }

    func openDoor() {
        yield waitForSeconds(2.0)
        setCollider(false)
    }
}
```

If the `Door` entity is destroyed while `openDoor` is suspended at the `yield`, the coroutine is cancelled at the next resume attempt. `setCollider(false)` never executes. No dangling coroutines.

---

## 8. Cancellation

**Automatic cancellation:** When an object is destroyed, the VM marks all its coroutines as cancelled. On the next frame, the scheduler skips cancelled coroutines instead of resuming them.

**Propagation:** Cancellation propagates to child coroutines. If `openDoor` launched a child coroutine via `yield someChild()`, that child is also cancelled.

**No explicit cancellation API in the initial implementation.** Cancellation is always lifetime-driven.

---

## 9. Coroutine Scheduler

The scheduler lives inside the VM and runs once per frame. It maintains a list of all active coroutines. Each frame:

1. For each coroutine in the list:
   - If cancelled: remove it
   - If waiting (time/frames): check if the wait condition is met
   - If ready: resume execution until the next `yield` or return

2. Completed coroutines are removed from the list

The scheduler is driven by the host calling `vm.tick(delta)` once per frame.

```rust
// Host game loop
vm.tick(delta_seconds);
```

---

## 10. Edge Cases

1. **Given** a coroutine calls `yield waitUntil(condition)` where the condition never becomes true, **then** the coroutine suspends indefinitely until the owning object is destroyed or the VM is shut down.
2. **Given** a coroutine yields another coroutine that itself yields, **then** the outer coroutine stays suspended until the inner coroutine returns — recursively.
3. **Given** a coroutine returns a value but the caller used `start` instead of `yield`, **then** the return value is discarded — `start` does not await completion.
4. **Given** a non-coroutine function calls `yield`, **then** compile error — `yield` is only valid inside coroutine functions.
5. **Given** `waitUntil` is given a lambda that captures a local variable, **then** the lambda keeps the captured variable alive until the condition fires or the coroutine is cancelled.

---

## 11. Performance Characteristics

| Operation                  | Cost                                                     |
|----------------------------|----------------------------------------------------------|
| `start coroutine()`        | Allocate coroutine stack frame, add to scheduler list    |
| `yield` (bare)             | Suspend current frame, scheduler moves to next coroutine |
| `yield waitForSeconds(n)`  | Record resume timestamp, suspend                         |
| `yield waitUntil(fn)`      | Call `fn` each frame until true, suspend between calls   |
| `yield anotherCoroutine()` | Chain dependency, suspend until child completes          |
| Cancellation               | Mark cancelled in O(1), removal on next tick             |

---

## 12. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
