---
title: Math
description: Math functions, constants, interpolation, and noise generation.
---

## Math

Module name: `"math"`

```writ
abs(-5.0)         // 5.0
ceil(1.2)         // 2.0
floor(1.9)        // 1.0
round(1.5)        // 2.0
sqrt(25.0)        // 5.0
pow(2.0, 10.0)    // 1024.0
log(100.0)        // 4.605...
exp(1.0)          // 2.718...

sin(PI / 2)       // 1.0
cos(0.0)          // 1.0
tan(PI / 4)       // 1.0

min(3.0, 5.0)     // 3.0
max(3.0, 5.0)     // 5.0
clamp(150.0, 0.0, 100.0)  // 100.0

// Constants
PI        // 3.14159...
TAU       // 6.28318...
INFINITY
```

---

## Interpolation

Module name: `"interpolation"`

```writ
lerp(0.0, 100.0, 0.5)         // 50.0
smoothstep(0.0, 1.0, t)
inverseLerp(0.0, 100.0, 50.0) // 0.5
```

---

## Noise

Module name: `"noise"`. Backed by `fastnoise-lite`.

```writ
noise2D(x, y)              // float in -1.0..1.0
noise3D(x, y, z)
```
