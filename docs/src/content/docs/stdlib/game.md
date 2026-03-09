---
title: Game
description: Math types, tweening, and timers for game development.
---

## Vector2 / Vector3 / Vector4

2D, 3D, and 4D vectors. Backed by `glam`.

```writ
let v2 = Vector2(x: 1.0, y: 0.0)
let v3 = Vector3(x: 0.0, y: 1.0, z: 0.0)

v2.length()
v2.normalized()
v2.dot(other)
v2.lerp(other, 0.5)

v3.cross(other)
v3.length()
v3.normalized()
```

---

## Matrix3 / Matrix4

3x3 and 4x4 matrices. Backed by `glam`.

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

```writ
let q = Quaternion_fromAxisAngle(axis, angle)
let q = Quaternion_fromEuler(x, y, z)
let q = Quaternion_lookRotation(forward, up)

q.normalized()
q.slerp(other, t)
q.toMatrix()
```

---

## Color

RGBA color.

```writ
let red = Color(r: 1.0, g: 0.0, b: 0.0, a: 1.0)
let c = Color.fromHex("#FF5733")
let c = Color.fromHSV(h, s, v)

c.lerp(other, 0.5)
```

---

## Rectangle / BoundingBox

2D rectangle and 3D bounding box.

```writ
let r = Rectangle(x: 0.0, y: 0.0, width: 100.0, height: 50.0)
r.contains(point)
r.intersects(other)
r.area()
```

---

## Tween

Module name: `"tween"`. Animate a value over time.

```writ
let t = Tween(from: 0.0, to: 100.0, duration: 2.0)
t.setEasing("easeInQuad")   // linear, easeInQuad, easeOutQuad, easeInOutQuad,
                             // easeInCubic, easeOutCubic, easeInOutCubic, smoothstep
t.setLoop("pingpong")       // "none", "loop", "pingpong"
t.setDelay(0.5)             // delay before starting

t.update(delta)
let value = t.value()
t.isFinished()
t.reset()
```

---

## Timer

Module name: `"timer"`.

```writ
let t = Timer(duration: 5.0)
t.start()
t.update(delta)
t.stop()
t.reset()

t.isFinished()
t.isRunning()
t.elapsed()
t.remaining()

t.setRepeating(true)
t.setCallback(myFunction)
```
