# ECS Design

> **Status:** Draft — API design complete, implementation pending

## 1. Overview

A custom ECS library built alongside the engine. Not a wrapper around an existing library. Designed for small to medium 2D games. Prioritizes a simple, consistent API over implementation simplicity — complexity lives in the library, not in user code.

### Stack

| Concern | Library |
|---|---|
| Windowing, input, audio, 2D rendering | SDL3 |
| UI layout | Clay |
| Physics | Rapier 2D |
| Math | glam |
| Texture loading | image crate |
| Scripting | Writ |

### Design principles

- Table-driven JSON content definition (items, enemies, spells, skills)
- Composition at load time via JSON component merging
- Flat entity/component store at runtime
- Rules engine: global, stateless, non-entity-based, Writ-authored
- Behavior trees: per-entity AI only
- DAG scheduler for system ordering

---

## 2. Entity IDs

- Type: `u64`, packed 32-bit index / 32-bit generation
- Each `World` is independent with its own ID space
- Multiple worlds supported
- Cross-world entity references are convention — the caller is responsible for knowing which world an ID belongs to

---

## 3. Storage

- **Uniform sparse-set** for all components
- Three parallel arrays per component pool: sparse array, dense entity array, dense component array
- **Paged sparse array** to avoid pointer invalidation on growth
- **Swap-and-pop** removal (no pointer stability by default)
- Structural changes are deferred via internal command buffer — never visible to the user
- Command buffer flushes automatically at the end of `schedule.run()` or explicitly via `world.flush()`

---

## 4. Full API

### Entity lifecycle

```rust
world.spawn() -> EntityId
world.despawn(entity)
world.is_alive(entity) -> bool
```

### Components

Any Rust struct with `impl Component for MyType {}`.

```rust
world.add_component(entity, value)
world.get_component::<T>(entity) -> Option<&T>
world.set_component(entity, value)
world.remove_component::<T>(entity)
world.has_component::<T>(entity) -> bool
```

### Tags

Zero-size marker structs. Same API as components — no special treatment needed. Storage never allocates a data array for zero-size types.

```rust
struct Frozen;
struct Player;

world.add_component(entity, Frozen)
world.has_component::<Frozen>(entity) -> bool
world.remove_component::<Frozen>(entity)
```

### Relations

Zero-size marker structs used as relation kinds. The target is an `EntityId`. Same naming convention as components.

```rust
struct ChildOf;
struct Equipped;

world.add_relation::<R>(entity, target)
world.get_relation::<R>(entity) -> Option<EntityId>   // one target
world.get_relations::<R>(entity) -> &[EntityId]       // many targets
world.remove_relation::<R>(entity)
world.has_relation::<R>(entity) -> bool
```

Backed by a dedicated relation index — not component storage. Gives O(1) lookup and fast traversal in both directions without full pool iteration.

### Resources

Global singletons. Live on the world. `insert` rather than `add` because resources replace — there is no concept of a second instance.

```rust
world.insert_resource(value)
world.get_resource::<T>() -> Option<&T>
world.remove_resource::<T>()
```

### Managed resources

Ref-counted, deduped by path, automatically disposed when ref count hits zero. The loader is registered once per type per extension. The ECS dispatches to the right loader based on file extension. Unknown extensions produce an error at component-add time.

```rust
// Registration — once at startup
world.register_resource::<Texture>("png", |path| sdl.load_texture(path));
world.register_resource::<Texture>("jpg", |path| sdl.load_texture(path));
world.register_resource::<Sound>("wav",   |path| sdl.load_sound(path));
world.register_resource::<Sound>("ogg",   |path| sdl.load_sound(path));

// Usage — just a component
world.add_component(entity, Resource::new("player.png"));   // loads as Texture
world.add_component(entity, Resource::new("footstep.wav")); // loads as Sound
world.add_component(entity, Resource::new("data.xyz"));     // error: unknown extension

// Cleanup — automatic on despawn
world.despawn(entity); // ref count decremented, freed if zero
```

### Queries

Typestate pattern enforces at least one filter before `.build()` is available. Calling `.build()` on an unfiltered query is a compile error.

```rust
// Query<Unfiltered> — .build() not available
// Query<Filtered>   — .build() available

Query::new(world)
    .with::<T>()              // must have T
    .with_all::<A, B>()       // must have all of A, B
    .with_one_of::<A, B>()    // must have at least one of A, B
    .without::<T>()           // must not have T
    .build() -> EntitySet
```

### Systems

A system is a struct implementing the provided `System` trait. Stateless by construction — no fields.

```rust
// Provided by the library
pub trait System {
    fn query(&self, world: &World) -> EntitySet;
    fn run(&self, entities: EntitySet, delta: f32);
}
```

The scheduler calls `query` to produce the `EntitySet`, then passes it directly into `run`. `delta` is measured by the user's game loop and passed into `schedule.run()` — the ECS never measures time.

```rust
struct MovementSystem;

impl System for MovementSystem {
    fn query(&self, world: &World) -> EntitySet {
        Query::new(world)
            .with::<Position>()
            .with::<Velocity>()
            .build()
    }

    fn run(&self, entities: EntitySet, delta: f32) {
        for entity in entities {
            let pos = entity.get_component::<Position>();
            let vel = entity.get_component::<Velocity>();
            pos.x += vel.x * delta;
            pos.y += vel.y * delta;
        }
    }
}
```

### Scheduler

Stages run in fixed order: `PreUpdate → Update → PostUpdate → Render`. Custom stages can be defined. Systems are sequential by default within a stage. `set_parallel` opts a stage into parallel execution. Dependencies are declared within a stage only.

```rust
// User owns delta — measured in the game loop
let delta = last.elapsed().as_secs_f32();

Schedule::new()
    .add_system(InputSystem,     Stage::PreUpdate, &[])
    .add_system(MovementSystem,  Stage::Update,    &[])
    .add_system(CollisionSystem, Stage::Update,    &[MovementSystem])
    .add_system(RenderSystem,    Stage::Render,    &[])
    .set_parallel(Stage::Update, true)
    .run(&mut world, delta) // flushes command buffer at end
```

### Events

Any Rust struct is an event — no marker trait required. Events are deferred and dispatched at the frame boundary. Subscription lifetime is managed by the returned handle — drop to unsubscribe. Multiple handlers for the same event are unordered.

```rust
world.fire(DamageDealt { entity, amount: 10.0 });

let _handle = world.on::<DamageDealt>(|event, world| { .. });
let _handle = world.on_add::<Health>(|entity, world| { .. });
let _handle = world.on_remove::<Health>(|entity, world| { .. });
```

### Serialization

Opt-in per component via the `ToDict` trait. The world produces a `HashMap<String, Value>`. The caller decides the format — JSON, BSON, or in-memory. The ECS has no opinion on wire format.

```rust
// Value enum — provided by the library
enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<Value>),
    Dict(HashMap<String, Value>),
}

// Opt-in per component
impl ToDict for MyComponent {
    fn to_dict(&self) -> HashMap<String, Value>;
    fn from_dict(dict: &HashMap<String, Value>) -> Self;
}

world.serialize()        -> HashMap<String, Value>
world.deserialize(dict)
```

---

## 5. Change Detection

Handled externally by the rules engine via command buffer events. The storage layer is dumb. The command buffer fires events at flush time. The rules engine subscribes via `world.on_add` / `world.on_remove`.

---

## 6. Writ Scripting

No Writ scripting layer over ECS internals. Writ scripts interact via the event and action API surface only. ECS internals are never exposed to scripts.

The `System` trait is provided as a globally available host type in Writ — no import needed:

```writ
class MovementSystem with System {
    func query(world: World) -> EntitySet {
        return Query.new(world)
            .with<Position>()
            .with<Velocity>()
            .build()
    }

    func run(entities: EntitySet, delta: float) {
        for entity in entities {
            let pos = entity.get<Position>()
            let vel = entity.get<Velocity>()
            pos.x += vel.x * delta
            pos.y += vel.y * delta
        }
    }
}
```

---

## 7. Open Questions

- Archetypes — almost certainly no given the sparse-set decision. To be confirmed.

---

## 8. Revision History

| Date       | Change |
|------------|--------|
| 2026-03-07 | Initial design — full API locked |
