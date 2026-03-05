# Reflection

> **Crate:** `writ-stdlib` | **Status:** Draft

## 1. Purpose

Reflection provides runtime introspection of Writ values — querying type names, checking field existence, reading and writing fields by string name, listing members, and invoking methods dynamically. This enables serialization, debug tools, data-driven systems, and generic programming patterns.

## 2. Dependencies

| Depends On                       | Relationship                                        |
|----------------------------------|-----------------------------------------------------|
| [type-system.md](type-system.md) | Reflection operates on the runtime type system      |
| [structs.md](structs.md)         | Structs are a primary target for reflection         |
| [stdlib.md](stdlib.md)           | Reflection is a stdlib module                       |

**Depended on by:** [lsp.md](../tooling/lsp.md)

---

## 3. Visibility Rules

Reflection respects the visibility system. Only `public` fields and methods are visible to reflection functions. Private and default-visibility members are hidden.

```writ
struct Config {
    public name: string = ""
    private secret: string = "hidden"
}

let c = Config(name: "test", secret: "key")
fields(c)            // ["name"] — secret is hidden
hasField(c, "secret") // false — private
```

---

## 4. Functions

### 4.1 Type Introspection

| Function     | Signature                        | Description                               |
|--------------|----------------------------------|-------------------------------------------|
| `typeof`     | `(value: any) -> string`        | Get the type name of a value as a string  |
| `instanceof` | `(value: any, typeName: string) -> bool` | Check if a value is an instance of a type |

```writ
let p = Point(x: 1.0, y: 2.0)
typeof(p)                    // "Point"
typeof(42)                   // "int"
typeof("hello")              // "string"
instanceof(p, "Point")       // true
instanceof(p, "string")      // false
```

`typeof` improves on the existing `type()` function by returning the actual type name for structs, classes, and enums instead of the generic `"object"`.

### 4.2 Field Introspection

| Function   | Signature                                  | Description                              |
|------------|-------------------------------------------|------------------------------------------|
| `hasField` | `(obj: any, name: string) -> bool`        | Check if object has a named public field |
| `getField` | `(obj: any, name: string) -> any`         | Read a public field by string name       |
| `setField` | `(obj: any, name: string, value: any)`    | Write a public field by string name      |
| `fields`   | `(obj: any) -> Array<string>`             | List all public field names              |

```writ
let p = Point(x: 1.0, y: 2.0)

hasField(p, "x")          // true
hasField(p, "z")          // false

getField(p, "x")          // 1.0
getField(p, "y")          // 2.0

fields(p)                  // ["x", "y"]
```

**Supported types:**
- Structs — field introspection on public fields
- Classes — field introspection on public fields (includes inherited)
- Enums — field introspection on public fields
- Dictionaries — keys act as field names

**`setField` limitation:** Does not work on struct values passed to native functions (value semantics — the copy is modified, not the original). Works on class instances (reference types) and dictionaries.

### 4.3 Method Introspection

| Function    | Signature                                   | Description                               |
|-------------|---------------------------------------------|-------------------------------------------|
| `methods`   | `(obj: any) -> Array<string>`               | List all public method names              |
| `hasMethod` | `(obj: any, name: string) -> bool`          | Check if type has a named public method   |
| `invoke`    | `(obj: any, name: string, ...args) -> any`  | Call a method by string name with args    |

```writ
struct Vector2 {
    public x: float = 0.0
    public y: float = 0.0

    func length() -> float {
        return sqrt(x * x + y * y)
    }
}

let v = Vector2(x: 3.0, y: 4.0)
methods(v)                   // ["length"]
hasMethod(v, "length")       // true
invoke(v, "length")          // 5.0
```

**`invoke` behavior:**
- Looks up the method by name on the value's type
- Passes additional arguments after the method name
- Returns the method's return value
- Errors if the method does not exist or arguments are wrong

---

## 5. Type Support Matrix

| Function     | Struct | Class | Enum | Dict | Primitives |
|--------------|--------|-------|------|------|------------|
| `typeof`     | yes    | yes   | yes  | yes  | yes        |
| `instanceof` | yes    | yes   | yes  | yes  | yes        |
| `hasField`   | yes    | yes   | yes  | yes  | no         |
| `getField`   | yes    | yes   | yes  | yes  | no         |
| `setField`   | no*    | yes   | yes  | yes  | no         |
| `fields`     | yes    | yes   | yes  | yes  | no         |
| `methods`    | yes    | yes   | yes  | no   | no         |
| `hasMethod`  | yes    | yes   | yes  | no   | no         |
| `invoke`     | yes    | yes   | yes  | no   | no         |

\* `setField` on structs does not modify the original due to value semantics.

---

## 6. Edge Cases

1. **Given** `getField(value, name)` where `name` is a private field, **then** runtime error — field not found.
2. **Given** `fields(42)` on a primitive, **then** returns empty array `[]`.
3. **Given** `invoke(obj, "unknownMethod")`, **then** runtime error — method not found.
4. **Given** `typeof(null)`, **then** returns `"null"`.
5. **Given** `instanceof(null, "null")`, **then** returns `true`.

---

## 7. Revision History

| Date       | Change        |
|------------|---------------|
| 2026-03-03 | Initial draft |
