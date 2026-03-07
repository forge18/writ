# Standard Library

> **Crate:** `writ-stdlib` | **Status:** Draft

## 1. Purpose

Writ's standard library provides universal utilities and game-dev primitives that every game script needs. The host provides engine-specific functionality (rendering, physics, audio, etc.). The standard library covers everything else — from basic I/O to spatial math, input abstractions, and animation utilities.

Every module is implemented as a thin scripting API over Rust's `std` where possible. External crates are used only when `std` genuinely cannot cover it.

## 2. Dependencies

| Depends On                       | Relationship                                |
|----------------------------------|---------------------------------------------|
| [type-system.md](type-system.md) | All stdlib functions are typed              |
| [vm.md](../runtime/vm.md)        | Stdlib is registered into the VM at startup |

| External Crate   | Used By                               |
|------------------|---------------------------------------|
| `glam`           | Vector, Matrix, Quaternion, Transform |
| `FastNoiseLite`  | Noise                                 |
| `rand`           | Random                                |

---

## 3. Modules

### 3.1 Basic

Always available. No import required.

| Function | Signature                            | Description                      |
|----------|--------------------------------------|----------------------------------|
| `print`  | `(value: string)`                    | Output to host console           |
| `assert` | `(condition: bool, message: string)` | Assert condition, error if false |
| `type`   | `(value: any) -> string`             | Get the type name of a value     |

---

### 3.2 Math

Backed by `std::f32` / `std::f64`.

| Function             | Description                |
|----------------------|----------------------------|
| `abs(x)`             | Absolute value             |
| `ceil(x)`            | Round up                   |
| `floor(x)`           | Round down                 |
| `round(x)`           | Round to nearest           |
| `sqrt(x)`            | Square root                |
| `sin(x)`             | Sine (radians)             |
| `cos(x)`             | Cosine (radians)           |
| `tan(x)`             | Tangent (radians)          |
| `min(a, b)`          | Minimum of two values      |
| `max(a, b)`          | Maximum of two values      |
| `clamp(x, min, max)` | Clamp value to range       |
| `pow(base, exp)`     | Exponentiation             |
| `log(x)`             | Natural logarithm          |
| `exp(x)`             | e raised to the power of x |

| Constant   | Value             |
|------------|-------------------|
| `PI`       | 3.14159…          |
| `TAU`      | 6.28318…          |
| `INFINITY` | Positive infinity |

---

### 3.3 String

Backed by `std::string::String` / `std::str`. Called as methods on string values.

| Method               | Description                            |
|----------------------|----------------------------------------|
| `.len()`             | Length in characters                   |
| `.trim()`            | Remove leading and trailing whitespace |
| `.trimStart()`       | Remove leading whitespace              |
| `.trimEnd()`         | Remove trailing whitespace             |
| `.toUpper()`         | Convert to uppercase                   |
| `.toLower()`         | Convert to lowercase                   |
| `.contains(s)`       | Check if contains substring            |
| `.startsWith(s)`     | Check if starts with prefix            |
| `.endsWith(s)`       | Check if ends with suffix              |
| `.replace(old, new)` | Replace all occurrences                |
| `.split(separator)`  | Split into `Array<string>`             |
| `.join(array)`       | Join array of strings                  |
| `.charAt(index)`     | Character at index                     |
| `.indexOf(s)`        | First index of substring               |
| `.parse()`           | Convert string to other types          |

---

### 3.4 Array

Backed by `std::vec::Vec`. Called as methods on array values.

| Method                 | Description                                    |
|------------------------|------------------------------------------------|
| `.push(item)`          | Append item                                    |
| `.pop()`               | Remove and return last item                    |
| `.insert(index, item)` | Insert at index                                |
| `.remove(index)`       | Remove at index                                |
| `.len()`               | Number of items                                |
| `.isEmpty()`           | True if length is 0                            |
| `.contains(item)`      | True if item is present                        |
| `.indexOf(item)`       | First index of item                            |
| `.reverse()`           | Reverse in place                               |
| `.sort()`              | Sort in place                                  |
| `.map(fn)`             | Return new array with fn applied to each item  |
| `.filter(fn)`          | Return new array with items matching predicate |
| `.reduce(fn, initial)` | Reduce to single value                         |
| `.first()`             | First item or `Optional<T>`                    |
| `.last()`              | Last item or `Optional<T>`                     |
| `.slice(start, end)`   | Return sub-array                               |

---

### 3.5 Dictionary

Backed by `std::collections::HashMap`. Called as methods on dictionary values.

| Method           | Description                                          |
|------------------|------------------------------------------------------|
| `.keys()`        | `Array<K>` of all keys                               |
| `.values()`      | `Array<V>` of all values                             |
| `.contains(key)` | True if key exists                                   |
| `.remove(key)`   | Remove entry                                         |
| `.len()`         | Number of entries                                    |
| `.isEmpty()`     | True if length is 0                                  |
| `.merge(other)`  | Merge another dictionary in (other wins on conflict) |

---

### 3.6 I/O

Backed by `std::io` / `std::fs`. Can be disabled by the host — mod scripts will typically not have access to this module.

| Function                   | Description                  |
|----------------------------|------------------------------|
| `readFile(path)`           | Read file contents as string |
| `writeFile(path, content)` | Write string to file         |
| `readLine()`               | Read a line from stdin       |
| `fileExists(path)`         | True if file exists          |

---

### 3.7 Time

Backed by `std::time`.

| Function             | Description                     |
|----------------------|---------------------------------|
| `now()`              | Current timestamp               |
| `elapsed(timestamp)` | Seconds elapsed since timestamp |

---

### 3.8 Random

Backed by the `rand` crate — the only external dependency in the standard library.

| Function                | Description                     |
|-------------------------|---------------------------------|
| `random()`              | Random float in 0.0..1.0        |
| `randomInt(min, max)`   | Random int in range (inclusive) |
| `randomFloat(min, max)` | Random float in range           |
| `shuffle(array)`        | Shuffle array in place          |

---

### 3.9 Reflection

Runtime introspection of values. See [reflection.md](reflection.md) for full specification.

| Function     | Signature                                        | Description                              |
|--------------|--------------------------------------------------|------------------------------------------|
| `typeof`     | `(value: any) -> string`                         | Get the type name of a value             |
| `instanceof` | `(value: any, typeName: string) -> bool`         | Check if value is an instance of a type  |
| `hasField`   | `(obj: any, name: string) -> bool`               | Check if object has a named public field |
| `getField`   | `(obj: any, name: string) -> any`                | Read a public field by string name       |
| `setField`   | `(obj: any, name: string, value: any)`           | Write a public field by string name      |
| `fields`     | `(obj: any) -> Array<string>`                    | List all public field names              |
| `methods`    | `(obj: any) -> Array<string>`                    | List all public method names             |
| `hasMethod`  | `(obj: any, name: string) -> bool`               | Check if type has a named public method  |
| `invoke`     | `(obj: any, name: string, ...args: any) -> any`  | Call a method by string name             |

Reflection respects visibility — only `public` fields and methods are visible.

---

### 3.10 Vector

Backed by `glam`. Operator overloading supported (`+`, `-`, `*`, `/`).

| Type       | Fields              | Description       |
|------------|---------------------|-------------------|
| `Vector2`  | `x`, `y`            | 2D vector (float) |
| `Vector3`  | `x`, `y`, `z`       | 3D vector (float) |
| `Vector4`  | `x`, `y`, `z`, `w`  | 4D vector (float) |

**Constructors:**

- `Vector2(x, y)`, `Vector3(x, y, z)`, `Vector4(x, y, z, w)`
- `Vector2.ZERO`, `Vector2.ONE`, `Vector2.UP`, `Vector2.DOWN`, `Vector2.LEFT`, `Vector2.RIGHT`
- `Vector3.ZERO`, `Vector3.ONE`, `Vector3.UP`, `Vector3.DOWN`, `Vector3.FORWARD`, `Vector3.BACK`

**Methods (shared across all vector types):**

| Method                    | Description                     |
|---------------------------|---------------------------------|
| `.length()`               | Magnitude                       |
| `.lengthSquared()`        | Squared magnitude (avoids sqrt) |
| `.normalized()`           | Unit vector                     |
| `.dot(other)`             | Dot product                     |
| `.distance(other)`        | Distance to another vector      |
| `.distanceSquared(other)` | Squared distance                |
| `.lerp(other, t)`         | Linear interpolation            |
| `.clamp(min, max)`        | Clamp each component            |
| `.abs()`                  | Absolute value of each component|
| `.sign()`                 | Sign of each component          |
| `.floor()`                | Floor each component            |
| `.ceil()`                 | Ceil each component             |
| `.round()`                | Round each component            |
| `.min(other)`             | Component-wise minimum          |
| `.max(other)`             | Component-wise maximum          |

**Vector3-specific:**

| Method          | Description  |
|-----------------|--------------|
| `.cross(other)` | Cross product|

**Operators:** `+`, `-`, `*` (scalar & component-wise), `/` (scalar & component-wise), `-` (unary negate), `==`, `!=`

---

### 3.11 Matrix

Backed by `glam`.

| Type      | Description                |
|-----------|----------------------------|
| `Matrix3` | 3x3 matrix (2D transforms) |
| `Matrix4` | 4x4 matrix (3D transforms) |

**Constructors:**

- `Matrix3.IDENTITY`, `Matrix3.ZERO`
- `Matrix4.IDENTITY`, `Matrix4.ZERO`

**Methods:**

| Method                  | Description                                 |
|-------------------------|---------------------------------------------|
| `.determinant()`        | Matrix determinant                          |
| `.inverse()`            | Inverse matrix                              |
| `.transpose()`          | Transposed matrix                           |
| `.multiply(other)`      | Matrix multiplication                       |
| `.transformPoint(vec)`  | Transform a point by this matrix            |
| `.transformVector(vec)` | Transform a direction (ignores translation) |

**Matrix3 factories:**

| Factory                       | Description            |
|-------------------------------|------------------------|
| `Matrix3.rotation(angle)`     | 2D rotation matrix     |
| `Matrix3.scale(sx, sy)`       | 2D scale matrix        |
| `Matrix3.translation(tx, ty)` | 2D translation matrix  |

**Matrix4 factories:**

| Factory                                                      | Description              |
|--------------------------------------------------------------|--------------------------|
| `Matrix4.rotation(axis, angle)`                              | 3D rotation around axis  |
| `Matrix4.scale(sx, sy, sz)`                                  | 3D scale matrix          |
| `Matrix4.translation(tx, ty, tz)`                            | 3D translation matrix    |
| `Matrix4.perspective(fov, aspect, near, far)`                | Perspective projection   |
| `Matrix4.orthographic(left, right, bottom, top, near, far)`  | Orthographic projection  |
| `Matrix4.lookAt(eye, target, up)`                            | View matrix              |

**Operators:** `*` (matrix-matrix, matrix-vector)

---

### 3.12 Quaternion

Backed by `glam`.

| Type         | Fields              | Description         |
|--------------|---------------------|---------------------|
| `Quaternion` | `x`, `y`, `z`, `w`  | Rotation quaternion |

**Constructors:**

- `Quaternion.IDENTITY`
- `Quaternion.fromAxisAngle(axis, angle)`
- `Quaternion.fromEuler(x, y, z)`
- `Quaternion.lookRotation(forward, up)`

**Methods:**

| Method             | Description                            |
|--------------------|----------------------------------------|
| `.normalized()`    | Unit quaternion                        |
| `.inverse()`       | Inverse rotation                       |
| `.dot(other)`      | Dot product                            |
| `.slerp(other, t)` | Spherical linear interpolation         |
| `.lerp(other, t)`  | Linear interpolation                   |
| `.toEuler()`       | Convert to euler angles (Vector3)      |
| `.toMatrix()`      | Convert to Matrix4                     |
| `.rotate(vec)`     | Rotate a Vector3                       |

**Operators:** `*` (quaternion-quaternion, quaternion-vector)

---

### 3.13 Transform

| Type          | Fields                                                         | Description  |
|---------------|----------------------------------------------------------------|--------------|
| `Transform2D` | `position: Vector2`, `rotation: float`, `scale: Vector2`       | 2D transform |
| `Transform3D` | `position: Vector3`, `rotation: Quaternion`, `scale: Vector3`  | 3D transform |

**Constructors:**

- `Transform2D()` / `Transform3D()` — identity
- `Transform2D(position, rotation, scale)`
- `Transform3D(position, rotation, scale)`

**Methods (shared):**

| Method                  | Description                    |
|-------------------------|--------------------------------|
| `.toMatrix()`           | Convert to Matrix3/Matrix4     |
| `.inverse()`            | Inverse transform              |
| `.transformPoint(vec)`  | Apply transform to a point     |
| `.transformVector(vec)` | Apply transform to a direction |

**Transform2D-specific:**

| Method               | Description                   |
|----------------------|-------------------------------|
| `.translate(offset)` | Move by offset                |
| `.rotate(angle)`     | Rotate by angle (radians)     |
| `.lookAt(target)`    | Rotate to face target Vector2 |

**Transform3D-specific:**

| Method                | Description                   |
|-----------------------|-------------------------------|
| `.translate(offset)`  | Move by offset                |
| `.rotate(axis, angle)`| Rotate around axis            |
| `.lookAt(target, up)` | Rotate to face target Vector3 |

---

### 3.14 Rectangle

| Type          | Fields                               | Description          |
|---------------|--------------------------------------|----------------------|
| `Rectangle`   | `position: Vector2`, `size: Vector2` | 2D axis-aligned rect |
| `BoundingBox` | `min: Vector3`, `max: Vector3`       | 3D axis-aligned box  |

**Constructors:**

- `Rectangle(x, y, width, height)`
- `Rectangle.fromPoints(min, max)`
- `BoundingBox(min, max)`

**Rectangle methods:**

| Method                 | Description                        |
|------------------------|------------------------------------|
| `.width()`             | Width                              |
| `.height()`            | Height                             |
| `.center()`            | Center point                       |
| `.area()`              | Area                               |
| `.contains(point)`     | True if point is inside            |
| `.intersects(other)`   | True if overlaps with another rect |
| `.intersection(other)` | Overlapping region or none         |
| `.merge(other)`        | Smallest rect containing both      |
| `.expand(amount)`      | Grow by amount on all sides        |

**BoundingBox methods:**

| Method                 | Description                   |
|------------------------|-------------------------------|
| `.size()`              | Dimensions as Vector3         |
| `.center()`            | Center point                  |
| `.volume()`            | Volume                        |
| `.contains(point)`     | True if point is inside       |
| `.intersects(other)`   | True if overlaps              |
| `.intersection(other)` | Overlapping region or none    |
| `.merge(other)`        | Smallest box containing both  |
| `.expand(amount)`      | Grow by amount on all sides   |

---

### 3.15 Color

Custom implementation. Stored as RGBA float (0.0–1.0).

| Type    | Fields             | Description |
|---------|--------------------|-------------|
| `Color` | `r`, `g`, `b`, `a` | RGBA color  |

**Constructors:**

- `Color(r, g, b)` — alpha defaults to 1.0
- `Color(r, g, b, a)`
- `Color.fromHex("#FF0000")` / `Color.fromHex("#FF0000FF")`
- `Color.fromHSV(h, s, v)`

**Constants:**

- `Color.WHITE`, `Color.BLACK`, `Color.RED`, `Color.GREEN`, `Color.BLUE`
- `Color.YELLOW`, `Color.CYAN`, `Color.MAGENTA`, `Color.TRANSPARENT`

**Methods:**

| Method            | Description                |
|-------------------|----------------------------|
| `.toHex()`        | Convert to hex string      |
| `.toHSV()`        | Convert to HSV (array)     |
| `.lerp(other, t)` | Interpolate between colors |
| `.lighten(amount)`| Lighten by amount (0.0–1.0)|
| `.darken(amount)` | Darken by amount (0.0–1.0) |
| `.inverted()`     | Invert RGB                 |
| `.withAlpha(a)`   | Return copy with new alpha |

**Operators:** `+`, `-`, `*` (scalar), `==`, `!=`

---

### 3.16 Interpolation

Custom implementation. Module-level functions.

| Function                                       | Description                           |
|------------------------------------------------|---------------------------------------|
| `lerp(a, b, t)`                                | Linear interpolation (float)          |
| `inverseLerp(a, b, value)`                     | Inverse lerp — returns t              |
| `remap(value, fromMin, fromMax, toMin, toMax)` | Remap value between ranges            |
| `smoothstep(a, b, t)`                          | Smooth hermite interpolation          |
| `smootherstep(a, b, t)`                        | Smoother (Ken Perlin's) interpolation |
| `slerp(a, b, t)`                               | Spherical lerp (for Quaternion)       |

**Easing functions** — all take `t` (0.0–1.0), return eased value:

| Function            | Description        |
|---------------------|--------------------|
| `easeInSine(t)`     | Sine ease in       |
| `easeOutSine(t)`    | Sine ease out      |
| `easeInOutSine(t)`  | Sine ease in-out   |
| `easeInQuad(t)`     | Quadratic ease in  |
| `easeOutQuad(t)`    | Quadratic ease out |
| `easeInOutQuad(t)`  | Quadratic in-out   |
| `easeInCubic(t)`    | Cubic ease in      |
| `easeOutCubic(t)`   | Cubic ease out     |
| `easeInOutCubic(t)` | Cubic in-out       |
| `easeInExpo(t)`     | Exponential in     |
| `easeOutExpo(t)`    | Exponential out    |
| `easeInOutExpo(t)`  | Exponential in-out |
| `easeInElastic(t)`  | Elastic ease in    |
| `easeOutElastic(t)` | Elastic ease out   |
| `easeInBounce(t)`   | Bounce ease in     |
| `easeOutBounce(t)`  | Bounce ease out    |

---

### 3.17 Noise

Backed by FastNoiseLite.

| Function                                  | Description                                        |
|-------------------------------------------|----------------------------------------------------|
| `noise2D(x, y)`                           | 2D noise (-1.0 to 1.0)                             |
| `noise3D(x, y, z)`                        | 3D noise (-1.0 to 1.0)                             |
| `noiseSeed(seed)`                         | Set noise seed                                     |
| `noiseType(type)`                         | Set type: "perlin", "simplex", "cellular", "value" |
| `noiseFractal(octaves, lacunarity, gain)` | Configure fractal settings                         |
| `noiseFrequency(freq)`                    | Set frequency                                      |

---

### 3.18 Tween

Custom implementation.

| Type    | Description                            |
|---------|----------------------------------------|
| `Tween` | Animates a value over time with easing |

**Constructor:**

- `Tween(from, to, duration)` — duration in seconds

**Methods:**

| Method                    | Description                              |
|---------------------------|------------------------------------------|
| `.setEasing(fn)`          | Set easing function (default: linear)    |
| `.setLoop(loop)`          | Set loop mode: "none", "loop", "pingpong"|
| `.setDelay(seconds)`      | Delay before starting                    |
| `.update(delta) -> float` | Advance by delta, return current value   |
| `.value()`                | Current interpolated value               |
| `.isFinished()`           | True if tween completed                  |
| `.reset()`                | Reset to beginning                       |

---

### 3.19 Timer

Custom implementation.

| Type    | Description                 |
|---------|-----------------------------|
| `Timer` | Countdown / repeating timer |

**Constructor:**

- `Timer(duration)` — duration in seconds

**Methods:**

| Method                | Description                  |
|-----------------------|------------------------------|
| `.start()`            | Start the timer              |
| `.stop()`             | Stop the timer               |
| `.reset()`            | Reset to initial duration    |
| `.update(delta)`      | Advance by delta seconds     |
| `.isFinished()`       | True if expired              |
| `.isRunning()`        | True if running              |
| `.remaining()`        | Seconds remaining            |
| `.elapsed()`          | Seconds elapsed              |
| `.setRepeating(bool)` | Auto-restart when finished   |
| `.setCallback(fn)`    | Function to call on finish   |

---

### 3.20 Input

Custom implementation. Enums and constants — no polling/state (that's host-provided).

**Keyboard:**

| Enum  | Values |
|-------|--------|
| `Key` | `A`–`Z`, `Num0`–`Num9`, `F1`–`F12`, `Space`, `Enter`, `Escape`, `Tab`, `Backspace`, `Delete`, `Insert`, `Home`, `End`, `PageUp`, `PageDown`, `Up`, `Down`, `Left`, `Right`, `LeftShift`, `RightShift`, `LeftCtrl`, `RightCtrl`, `LeftAlt`, `RightAlt` |

**Mouse:**

| Enum          | Values                                       |
|---------------|----------------------------------------------|
| `MouseButton` | `Left`, `Right`, `Middle`, `Back`, `Forward` |

**Game Controller:**

| Enum               | Values |
|--------------------|--------|
| `ControllerButton` | `A`, `B`, `X`, `Y`, `DPadUp`, `DPadDown`, `DPadLeft`, `DPadRight`, `LeftBumper`, `RightBumper`, `LeftStick`, `RightStick`, `Start`, `Back`, `Guide` |
| `ControllerAxis`   | `LeftStickX`, `LeftStickY`, `RightStickX`, `RightStickY`, `LeftTrigger`, `RightTrigger` |

These enums provide a standard vocabulary. The host maps its input system to these enums and registers functions like `isKeyPressed(key: Key) -> bool`.

---

## 4. Host Disabling Modules

The host can disable specific modules at VM construction time. I/O is the primary candidate — untrusted mod scripts should not have filesystem access.

```rust
let vm = VM::new()
    .disable_module("io")
    .register_type::<Player>()
```

Attempting to use a disabled module function results in a runtime error.

---

## 5. Revision History

| Date       | Change                                                    |
|------------|-----------------------------------------------------------|
| 2026-03-02 | Initial draft                                             |
| 2026-03-06 | Add game-dev modules: Vector, Matrix, Quaternion, Transform, Rectangle, Color, Interpolation, Noise, Tween, Timer, Input |
