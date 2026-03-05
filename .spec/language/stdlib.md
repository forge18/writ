# Standard Library

> **Crate:** `writ-stdlib` | **Status:** Draft

## 1. Purpose

Writ's standard library follows Lua's philosophy — small, complete, and universal. It provides only what every script needs regardless of host. The host provides domain-specific functionality. The standard library covers the rest.

Every module is implemented as a thin scripting API over Rust's `std` where possible. External crates are used only when `std` genuinely cannot cover it.

## 2. Dependencies

| Depends On                       | Relationship                                |
|----------------------------------|---------------------------------------------|
| [type-system.md](type-system.md) | All stdlib functions are typed              |
| [vm.md](../runtime/vm.md)        | Stdlib is registered into the VM at startup |

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

| Date       | Change        |
|------------|---------------|
| 2026-03-02 | Initial draft |
