---
title: Core
description: Basic global functions and runtime reflection.
---

## Basic

Global functions always available.

| Function | Signature                            | Description                 |
|----------|--------------------------------------|-----------------------------|
| `print`  | `(value: any)`                       | Print to the host console   |
| `assert` | `(condition: bool, msg: string)`     | Error if condition is false |

---

## Reflection

Module name: `"reflect"`

| Function     | Signature                                            | Description              |
|--------------|------------------------------------------------------|--------------------------|
| `typeof`     | `(value: any) -> string`                             | Type name of a value     |
| `instanceof` | `(value: any, type: string) -> bool`                 | Check type at runtime    |
| `hasField`   | `(value: any, field: string) -> bool`                | Check if a field exists  |
| `getField`   | `(value: any, field: string) -> any`                 | Get a field by name      |
| `setField`   | `(value: any, field: string, val: any)`              | Set a field by name      |
| `fields`     | `(value: any) -> Array<string>`                      | All field names          |
| `methods`    | `(value: any) -> Array<string>`                      | All method names         |
| `hasMethod`  | `(value: any, method: string) -> bool`               | Check if a method exists |
| `invoke`     | `(value: any, method: string, ...args: any) -> any`  | Call a method by name    |
