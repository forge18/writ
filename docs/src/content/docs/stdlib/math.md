---
title: Math
description: Math functions, constants, interpolation, and noise generation.
---

## Math

Module name: `"math"`

| Function | Signature                                      | Description          |
|----------|-------------------------------------------------|----------------------|
| `abs`    | `(n: float) -> float`                           | Absolute value       |
| `ceil`   | `(n: float) -> float`                           | Round up             |
| `floor`  | `(n: float) -> float`                           | Round down           |
| `round`  | `(n: float) -> float`                           | Round to nearest     |
| `sqrt`   | `(n: float) -> float`                           | Square root          |
| `pow`    | `(base: float, exp: float) -> float`            | Exponentiation       |
| `log`    | `(n: float) -> float`                           | Natural logarithm    |
| `exp`    | `(n: float) -> float`                           | e raised to n        |
| `sin`    | `(n: float) -> float`                           | Sine                 |
| `cos`    | `(n: float) -> float`                           | Cosine               |
| `tan`    | `(n: float) -> float`                           | Tangent              |
| `min`    | `(a: float, b: float) -> float`                 | Minimum of two       |
| `max`    | `(a: float, b: float) -> float`                 | Maximum of two       |
| `clamp`  | `(val: float, lo: float, hi: float) -> float`   | Clamp to range       |

```writ
abs(-5.0)         // 5.0
ceil(1.2)         // 2.0
floor(1.9)        // 1.0
round(1.5)        // 2.0
sqrt(25.0)        // 5.0
pow(2.0, 10.0)    // 1024.0

sin(PI / 2)       // 1.0
cos(0.0)          // 1.0
clamp(150.0, 0.0, 100.0)  // 100.0

// Constants
PI        // 3.14159...
TAU       // 6.28318...
INFINITY
```

---

## Interpolation

Module name: `"interpolation"`

| Function      | Signature                                          | Description                    |
|---------------|----------------------------------------------------|---------------------------------|
| `lerp`        | `(a: float, b: float, t: float) -> float`          | Linear interpolation           |
| `smoothstep`  | `(edge0: float, edge1: float, x: float) -> float`  | Smooth Hermite curve           |
| `inverseLerp` | `(a: float, b: float, value: float) -> float`      | Inverse linear interpolation   |

```writ
lerp(0.0, 100.0, 0.5)         // 50.0
smoothstep(0.0, 1.0, t)
inverseLerp(0.0, 100.0, 50.0) // 0.5
```

---

## Noise

Module name: `"noise"`. Backed by `fastnoise-lite`.

| Function  | Signature                                    | Description              |
|-----------|----------------------------------------------|--------------------------|
| `noise2D` | `(x: float, y: float) -> float`              | 2D noise in -1.0..1.0   |
| `noise3D` | `(x: float, y: float, z: float) -> float`    | 3D noise in -1.0..1.0   |

```writ
noise2D(x, y)              // float in -1.0..1.0
noise3D(x, y, z)
```
