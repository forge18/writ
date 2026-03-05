# Structs

> **Crate:** `writ-parser`, `writ-types`, `writ-compiler`, `writ-vm` | **Status:** Draft

## 1. Purpose

Structs are lightweight, value-type data containers. They provide a simpler alternative to classes for data that should be copied on assignment rather than shared by reference. Structs are ideal for small, composable data like vectors, colors, rectangles, and configuration bundles.

## 2. Dependencies

| Depends On                       | Relationship                                    |
|----------------------------------|-------------------------------------------------|
| [syntax.md](syntax.md)           | Struct syntax builds on existing field/method syntax |
| [type-system.md](type-system.md) | Structs are a new user-defined type category    |

**Depended on by:** [vm.md](../runtime/vm.md), [reflection.md](reflection.md), [lsp.md](../tooling/lsp.md)

---

## 3. Syntax

```writ
struct Point {
    public x: float = 0.0
    public y: float = 0.0

    func length() -> float {
        return sqrt(x * x + y * y)
    }
}
```

### 3.1 Fields

Fields use the same syntax as class fields — visibility modifier, name, type annotation, optional default value, optional setter.

```writ
struct Color {
    public r: float = 0.0
    public g: float = 0.0
    public b: float = 0.0
    public a: float = 1.0
        set(value) { field = clamp(value, 0.0, 1.0) }
}
```

### 3.2 Methods

Methods use the same syntax as class methods. `self` is implicitly available inside instance methods.

```writ
struct Vector2 {
    public x: float = 0.0
    public y: float = 0.0

    func length() -> float {
        return sqrt(x * x + y * y)
    }

    func normalized() -> Vector2 {
        let len = length()
        return Vector2(x: x / len, y: y / len)
    }

    static func zero() -> Vector2 {
        return Vector2(x: 0.0, y: 0.0)
    }
}
```

### 3.3 Constructors

Auto-generated from field declarations, identical to class constructors.

```writ
let p1 = Point()                         // defaults
let p2 = Point(x: 10.0, y: 20.0)       // named
let p3 = Point(10.0, 20.0)             // positional
```

---

## 4. Semantics

### 4.1 Value Type

Structs are **value types** — assignment copies the entire struct. This is the fundamental difference from classes.

```writ
var a = Point(x: 1.0, y: 2.0)
var b = a           // b is a copy of a
b.x = 99.0         // a.x is still 1.0
```

### 4.2 Structural Equality

Two struct values are equal if all their fields are equal.

```writ
let a = Point(x: 1.0, y: 2.0)
let b = Point(x: 1.0, y: 2.0)
assert(a == b, "same fields means equal")  // passes
```

### 4.3 No Inheritance

Structs do not support `extends`. This is a compile error:

```writ
struct Child extends Parent { }  // ERROR: structs cannot inherit
```

### 4.4 No Traits

Structs do not support `with`. This is a compile error:

```writ
struct Data with Serializable { }  // ERROR: structs cannot implement traits
```

---

## 5. Comparison with Classes

| Feature            | Class                     | Struct                   |
|--------------------|---------------------------|--------------------------|
| Semantics          | Reference (shared)        | Value (copied on assign) |
| Inheritance        | Yes (`extends`)           | No                       |
| Traits             | Yes (`with`)              | No                       |
| Fields             | Yes                       | Yes                      |
| Methods            | Yes                       | Yes                      |
| Setters            | Yes                       | Yes                      |
| Static methods     | Yes                       | Yes                      |
| Constructors       | Auto-generated            | Auto-generated           |
| `self` in methods  | Yes                       | Yes                      |
| Equality           | Reference identity        | Structural (field-by-field) |
| Export/Import      | Yes                       | Yes                      |

---

## 6. Usage

Structs can be used as field types, parameter types, return types, and collection element types.

```writ
class Player {
    public position: Vector2 = Vector2()
    public color: Color = Color(r: 1.0)

    func move(delta: Vector2) {
        position = Vector2(
            x: position.x + delta.x,
            y: position.y + delta.y
        )
    }
}

func distance(a: Point, b: Point) -> float {
    let dx = a.x - b.x
    let dy = a.y - b.y
    return sqrt(dx * dx + dy * dy)
}

let points: Array<Point> = [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 4.0)]
```

---

## 7. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-03 | Initial draft |
