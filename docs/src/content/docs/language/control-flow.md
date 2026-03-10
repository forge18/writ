---
title: Control Flow
description: Conditionals, pattern matching with when, and loops.
---

## if / else

```writ
if health <= 0 {
    die()
} else if health < 25 {
    playLowHealthWarning()
} else {
    heal()
}
```

### Type checking with `is`

Use `expr is TypeName` as a boolean condition:

```writ
if entity is Player {
    entity.takeDamage(10.0)
}

let isEnemy = obj is Enemy
let either = a is Player || a is NPC
```

---

## Ternary

```writ
let status = health > 0 ? "alive" : "dead"
```

---

## when

Pattern matching with exhaustiveness checking. Two forms:

### Value matching

```writ
when health {
    0          => print("Dead")
    1, 2, 3    => print("Critical")
    0..25      => print("Low")
    26..=100   => print("OK")
    else       => print("Overheal")
}
```

### Type matching

```writ
when result {
    is Success(value) => print(value)
    is Error(msg)     => print("Error: " .. msg)
}
```

### Guard clauses

```writ
when health {
    x if x < 0    => print("Invalid")
    x if x < 25   => print("Critical: $x")
    else           => print("OK")
}
```

### Multi-line arms

```writ
when result {
    is Success(value) => {
        print(value)
        log(value)
    }
    is Error(msg) => print(msg)
}
```

### Without a subject

Replaces `if`/`else` chains:

```writ
when {
    health == 100 => print("Full")
    health <= 0   => print("Dead")
    else          => print("Damaged")
}
```

---

## Loops

### while

```writ
while health > 0 {
    tick()
}
```

### for over a collection

```writ
for item in inventory {
    print(item.name)
}
```

### for over a range

```writ
for i in 0..10 {
    print(i)   // 0 to 9
}

for i in 0..=10 {
    print(i)   // 0 to 10
}
```

### Loop control

```writ
break     // exit loop
continue  // skip to next iteration
```
