---
title: Sandboxing & Security
description: Instruction limits, module disabling, type isolation, and the zero-access binding model.
---

## Zero-access by default

The VM starts with zero external access. The host explicitly opts in to what scripts can use:

```rust
let mut vm = Writ::new();
vm.register_type("Player", |args| { /* ... */ Ok(Box::new(player)) });
vm.register_host_fn("move", vec![Type::Int], Type::Void, fn1(player_move));
vm.register_host_fn("spawn", vec![Type::Str], Type::Void, fn1(entity_spawn));
```

Different VM instances can have different capabilities — mod scripts get a restricted API surface, core game scripts get full access. Same language, same VM, same execution model. The difference is purely what the host registers.

---

## Instruction limits

Protect against infinite loops in untrusted scripts. Configurable per VM instance:

```rust
vm.set_instruction_limit(1_000_000);  // kill after N instructions
```

When the limit is hit, the script is terminated and an error is returned to the host.

---

## Disabling stdlib modules

Remove entire standard library modules from a VM instance:

```rust
vm.disable_module("io");      // no file access
vm.disable_module("noise");   // no noise generation
```

See the [Standard Library](/writ/stdlib/core/) section for module names.

---

## Type isolation

Reset script-defined types between executions. Useful for evaluating untrusted one-off scripts:

```rust
vm.reset_script_types();
```

---

## Type checking toggle

Type checking is on by default. You can disable it:

```rust
vm.disable_type_checking(); // skip type checking
vm.enable_type_checking();  // re-enable
```

Disabling is useful when running scripts that call host functions registered without type info. All other behavior is unchanged.
