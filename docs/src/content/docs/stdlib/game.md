---
title: Game
description: Math types, tweening, and timers for game development.
---

## Vector2 / Vector3 / Vector4

2D, 3D, and 4D vectors. Backed by `glam`.

| Method       | Signature                              | Description            |
|--------------|----------------------------------------|------------------------|
| `length`     | `() -> float`                          | Magnitude              |
| `normalized` | `() -> VecN`                           | Unit vector            |
| `dot`        | `(other: VecN) -> float`              | Dot product            |
| `lerp`       | `(other: VecN, t: float) -> VecN`     | Linear interpolation   |
| `cross`      | `(other: Vec3) -> Vec3`               | Cross product (3D only)|

```writ
let v2 = Vector2(x: 1.0, y: 0.0)
let v3 = Vector3(x: 0.0, y: 1.0, z: 0.0)

v2.length()
v2.normalized()
v2.dot(other)
v2.lerp(other, 0.5)

v3.cross(other)
```

---

## Matrix3 / Matrix4

3x3 and 4x4 matrices. Backed by `glam`.

| Method      | Signature                                          | Description        |
|-------------|----------------------------------------------------|--------------------|
| `multiply`  | `(other: MatN) -> MatN`                            | Matrix multiply    |
| `inverse`   | `() -> MatN`                                       | Inverse matrix     |
| `transpose` | `() -> MatN`                                       | Transpose matrix   |

```writ
let m = Matrix4_IDENTITY
let rot = Matrix4_rotation(axis, angle)
let scale = Matrix4_scale(sx, sy, sz)
let trans = Matrix4_translation(tx, ty, tz)

m.multiply(other)
m.inverse()
m.transpose()
```

---

## Quaternion

Rotation representation. Backed by `glam`.

| Method              | Signature                                       | Description            |
|---------------------|-------------------------------------------------|------------------------|
| `fromAxisAngle`     | `(axis: Vec3, angle: float) -> Quaternion`      | From axis + angle      |
| `fromEuler`         | `(x: float, y: float, z: float) -> Quaternion`  | From Euler angles      |
| `lookRotation`      | `(forward: Vec3, up: Vec3) -> Quaternion`       | Look rotation          |
| `normalized`        | `() -> Quaternion`                              | Unit quaternion        |
| `slerp`             | `(other: Quaternion, t: float) -> Quaternion`   | Spherical lerp         |
| `toMatrix`          | `() -> Matrix4`                                 | Convert to matrix      |

```writ
let q = Quaternion_fromAxisAngle(axis, angle)
let q = Quaternion_fromEuler(x, y, z)

q.normalized()
q.slerp(other, t)
q.toMatrix()
```

---

## Color

RGBA color.

| Method    | Signature                                          | Description          |
|-----------|----------------------------------------------------|----------------------|
| `fromHex` | `(hex: string) -> Color`                           | From hex string      |
| `fromHSV` | `(h: float, s: float, v: float) -> Color`          | From HSV values      |
| `lerp`    | `(other: Color, t: float) -> Color`                | Interpolate colors   |

```writ
let red = Color(r: 1.0, g: 0.0, b: 0.0, a: 1.0)
let c = Color.fromHex("#FF5733")
let c = Color.fromHSV(h, s, v)

c.lerp(other, 0.5)
```

---

## Rectangle / BoundingBox

2D rectangle and 3D bounding box.

| Method       | Signature                           | Description            |
|--------------|-------------------------------------|------------------------|
| `contains`   | `(point: VecN) -> bool`            | Point inside check     |
| `intersects` | `(other: Self) -> bool`            | Overlap check          |
| `area`       | `() -> float`                      | Area (2D) or volume (3D) |

```writ
let r = Rectangle(x: 0.0, y: 0.0, width: 100.0, height: 50.0)
r.contains(point)
r.intersects(other)
r.area()
```

---

## Tween

Module name: `"tween"`. Animate a value over time.

| Method        | Signature                    | Description              |
|---------------|------------------------------|--------------------------|
| `setEasing`   | `(name: string)`             | Set easing function      |
| `setLoop`     | `(mode: string)`             | Set loop mode            |
| `setDelay`    | `(seconds: float)`           | Delay before start       |
| `update`      | `(delta: float)`             | Advance by delta         |
| `value`       | `() -> float`                | Current interpolated value |
| `isFinished`  | `() -> bool`                 | True when complete       |
| `reset`       | `()`                         | Reset to start           |

```writ
let t = Tween(from: 0.0, to: 100.0, duration: 2.0)
t.setEasing("easeInQuad")   // linear, easeInQuad, easeOutQuad, easeInOutQuad,
                             // easeInCubic, easeOutCubic, easeInOutCubic, smoothstep
t.setLoop("pingpong")       // "none", "loop", "pingpong"
t.setDelay(0.5)

t.update(delta)
let value = t.value()
t.isFinished()
```

---

## Timer

Module name: `"timer"`.

| Method         | Signature                    | Description              |
|----------------|------------------------------|--------------------------|
| `start`        | `()`                         | Start the timer          |
| `update`       | `(delta: float)`             | Advance by delta         |
| `stop`         | `()`                         | Stop the timer           |
| `reset`        | `()`                         | Reset to initial state   |
| `isFinished`   | `() -> bool`                 | True when elapsed >= duration |
| `isRunning`    | `() -> bool`                 | True while running       |
| `elapsed`      | `() -> float`                | Seconds elapsed          |
| `remaining`    | `() -> float`                | Seconds remaining        |
| `setRepeating` | `(repeat: bool)`             | Auto-reset on finish     |
| `setCallback`  | `(fn: () -> void)`           | Called on finish         |

```writ
let t = Timer(duration: 5.0)
t.start()
t.update(delta)

t.isFinished()
t.elapsed()
t.remaining()

t.setRepeating(true)
t.setCallback(myFunction)
```
