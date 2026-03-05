# Register-Based VM

> **Crate:** `writ-vm`, `writ-compiler` | **Status:** Draft

## 1. Purpose

This spec defines the register-based execution model for the Writ VM. It replaces the stack-based model to eliminate the three dominant performance bottlenecks identified in profiling: `drop_in_place<Value>` (10-38%), `Value::cheap_clone` (10-22%), and `Vec::push/pop` (1-4%).

## 2. Dependencies

| Depends On | Relationship |
|---|---|
| [vm.md](vm.md) | Extends the VM execution model |
| [coroutines.md](coroutines.md) | Yield/resume adapted for register placement |
| [rust-interop.md](rust-interop.md) | FFI calling convention unchanged |

---

## 3. Register Model

Registers are frame-relative slots in the existing `Vec<Value>` operand stack. Each function frame owns a contiguous window `[base..base+max_registers)`. The window is partitioned as:

```
┌─────────┬────────┬─────────────┐
│ params  │ locals │ temporaries │
│ 0..arity│        │             │
└─────────┴────────┴─────────────┘
```

- **Params:** Registers `0..arity` hold function parameters, placed by the caller.
- **Locals:** Sequential registers allocated by `let`/`var`/`const` declarations.
- **Temporaries:** Short-lived registers for intermediate expression values, allocated and freed with stack discipline.

All register operands are `u8` (0-255), interpreted as offsets from `CallFrame::base`. This matches the existing 256-local limit.

### 3.1 Stack Extension on Call

On function entry, the VM extends the stack to `base + max_registers`, filling new slots with `Value::Null`. On function return, the stack is shrunk back.

### 3.2 Value Type

`Value` remains unchanged: a 24-byte non-Copy enum with 14 variants. The register model eliminates cloning by reading values by reference from their register positions rather than pushing clones onto the stack.

---

## 4. Instruction Set

All instructions carry explicit register operands. The enum remains `#[derive(Clone, Copy)]` for LLVM jump table dispatch.

### 4.1 Notation

- `R(x)` = register at slot `x` (absolute: `stack[base + x]`)
- `K(x)` = constant from pool at index `x`
- `dst`, `src`, `a`, `b` = `u8` register indices

### 4.2 Load/Store

| Instruction | Semantics |
|---|---|
| `LoadInt(dst, i32)` | `R(dst) = I32(imm)` |
| `LoadConstInt(dst, u16)` | `R(dst) = I64(pool[idx])` |
| `LoadFloat(dst, f32)` | `R(dst) = F32(imm)` |
| `LoadConstFloat(dst, u16)` | `R(dst) = F64(pool[idx])` |
| `LoadBool(dst, bool)` | `R(dst) = Bool(imm)` |
| `LoadStr(dst, u32)` | `R(dst) = Str(Rc::clone(pool[idx]))` |
| `LoadNull(dst)` | `R(dst) = Null` |
| `Move(dst, src)` | `R(dst) = R(src).cheap_clone()` |
| `LoadGlobal(dst, u32)` | `R(dst) = globals[hash]` |

### 4.3 Arithmetic (Three-Address)

| Instruction | Semantics |
|---|---|
| `Add(dst, a, b)` | `R(dst) = R(a) + R(b)` — generic, quickenable |
| `Sub(dst, a, b)` | `R(dst) = R(a) - R(b)` — generic, quickenable |
| `Mul(dst, a, b)` | `R(dst) = R(a) * R(b)` — generic, quickenable |
| `Div(dst, a, b)` | `R(dst) = R(a) / R(b)` — generic, quickenable |
| `Mod(dst, a, b)` | `R(dst) = R(a) % R(b)` |
| `AddInt(dst, a, b)` | Typed: both operands known int |
| `SubInt(dst, a, b)` | Typed: both operands known int |
| `MulInt(dst, a, b)` | Typed: both operands known int |
| `DivInt(dst, a, b)` | Typed: both operands known int |
| `AddFloat(dst, a, b)` | Typed: both operands known float |
| `SubFloat(dst, a, b)` | Typed: both operands known float |
| `MulFloat(dst, a, b)` | Typed: both operands known float |
| `DivFloat(dst, a, b)` | Typed: both operands known float |
| `AddIntImm(dst, src, i32)` | `R(dst) = R(src) + I32(imm)` — fused load+add |
| `SubIntImm(dst, src, i32)` | `R(dst) = R(src) - I32(imm)` — fused load+sub |
| `Concat(dst, a, b)` | `R(dst) = str(R(a)) ++ str(R(b))` |
| `NullCoalesce(dst, a, b)` | `R(dst) = R(a) ?? R(b)` |

### 4.4 Unary

| Instruction | Semantics |
|---|---|
| `Neg(dst, src)` | `R(dst) = -R(src)` |
| `Not(dst, src)` | `R(dst) = !R(src)` |

### 4.5 Comparison (Three-Address)

| Instruction | Semantics |
|---|---|
| `Eq(dst, a, b)` | `R(dst) = R(a) == R(b)` — generic, quickenable |
| `Ne(dst, a, b)` | `R(dst) = R(a) != R(b)` — generic, quickenable |
| `Lt(dst, a, b)` | `R(dst) = R(a) < R(b)` — generic, quickenable |
| `Le(dst, a, b)` | `R(dst) = R(a) <= R(b)` — generic, quickenable |
| `Gt(dst, a, b)` | `R(dst) = R(a) > R(b)` — generic, quickenable |
| `Ge(dst, a, b)` | `R(dst) = R(a) >= R(b)` — generic, quickenable |
| `EqInt(dst, a, b)` | Typed int variants |
| ... | (all typed int/float variants follow the same pattern) |

### 4.6 Control Flow

| Instruction | Semantics |
|---|---|
| `Jump(i32)` | `ip += offset` |
| `JumpIfFalsy(src, i32)` | `if R(src) is falsy: ip += offset` |
| `JumpIfTruthy(src, i32)` | `if R(src) is truthy: ip += offset` |
| `TestLtInt(a, b, i32)` | `if !(R(a) <_int R(b)): ip += offset` — fused cmp+jump |
| `TestLeInt(a, b, i32)` | `if !(R(a) <=_int R(b)): ip += offset` |
| `TestGtInt(a, b, i32)` | `if !(R(a) >_int R(b)): ip += offset` |
| `TestGeInt(a, b, i32)` | `if !(R(a) >=_int R(b)): ip += offset` |
| `TestEqInt(a, b, i32)` | `if !(R(a) ==_int R(b)): ip += offset` |
| `TestNeInt(a, b, i32)` | `if !(R(a) !=_int R(b)): ip += offset` |
| `TestLtIntImm(a, i32, i32)` | `if !(R(a) <_int imm): ip += offset` |
| `TestLeIntImm(a, i32, i32)` | `if !(R(a) <=_int imm): ip += offset` |
| `TestGtIntImm(a, i32, i32)` | `if !(R(a) >_int imm): ip += offset` |
| `TestGeIntImm(a, i32, i32)` | `if !(R(a) >=_int imm): ip += offset` |
| `TestLtFloat(a, b, i32)` | Float variants |
| `TestLeFloat(a, b, i32)` | Float variants |
| `TestGtFloat(a, b, i32)` | Float variants |
| `TestGeFloat(a, b, i32)` | Float variants |

**Note:** Test instructions use inverted condition + forward jump. The "true" path falls through (no jump), which is the common case for loop conditions.

### 4.7 Function Calls

| Instruction | Semantics |
|---|---|
| `Call(base_reg, arg_count)` | Dynamic call: `R(base_reg)` is callee, args in `R(base_reg+1)..R(base_reg+1+N)`. Result written to `R(base_reg)`. |
| `CallDirect(base_reg, u16, arg_count)` | Static call: func_idx from function table. Args in `R(base_reg)..R(base_reg+N)`. Result written to `R(base_reg)`. |
| `CallMethod(base_reg, u32, arg_count)` | Method call: `R(base_reg)` is receiver. Args in `R(base_reg+1)..`. Result in `R(base_reg)`. |
| `Return(src)` | Return `R(src)` to caller's result register. |
| `ReturnNull` | Return `Null` to caller's result register. |

#### Calling Convention

**Caller side:**
1. Place arguments in consecutive registers starting at `base_reg` (for CallDirect) or `base_reg+1` (for Call/CallMethod where `base_reg` holds the callee/receiver).
2. Emit `CallDirect(base_reg, func_idx, arity)` or `Call(base_reg, arity)`.
3. After the call returns, the result is in `R(base_reg)`.

**Callee side:**
1. VM extends stack to `callee_base + max_registers`.
2. Parameters are in `R(0)..R(arity)` (the caller's argument registers become the callee's parameter registers).
3. On `Return(src)`, the VM writes `R(src)` to the caller's result register.

**CallDirect layout:**
```
Caller frame:
  R(base_reg)   = arg0  ← becomes callee's R(0)
  R(base_reg+1) = arg1  ← becomes callee's R(1)
  ...

Callee frame (base = caller.base + base_reg):
  R(0) = arg0
  R(1) = arg1
  ...
  R(arity..max_registers) = locals + temps (initialized to Null)
```

**Call (dynamic) layout:**
```
Caller frame:
  R(base_reg)   = callee_value  ← overwritten with result
  R(base_reg+1) = arg0  ← becomes callee's R(0)
  R(base_reg+2) = arg1  ← becomes callee's R(1)
  ...

Callee frame (base = caller.base + base_reg + 1):
  R(0) = arg0
  R(1) = arg1
  ...
```

### 4.8 Closures and Upvalues

| Instruction | Semantics |
|---|---|
| `LoadUpvalue(dst, u8)` | `R(dst) = upvalue_cell[idx].borrow().clone()` |
| `StoreUpvalue(u8, src)` | `*upvalue_cell[idx].borrow_mut() = R(src).cheap_clone()` |
| `MakeClosure(dst, u16)` | `R(dst) = Closure(func_idx, captured_upvalues)` |
| `CloseUpvalue(reg)` | Close upvalue at register `reg` (move stack value to heap cell) |

Upvalue mechanics are unchanged from the stack-based model. The `open_upvalues: HashMap<usize, Rc<RefCell<Value>>>` continues to use absolute stack positions (`base + reg`).

### 4.9 Collections

| Instruction | Semantics |
|---|---|
| `MakeArray(dst, start, count)` | `R(dst) = Array(R(start)..R(start+count))` |
| `MakeDict(dst, start, count)` | `R(dst) = Dict from R(start)..R(start+2*count)` (key/value pairs) |
| `GetIndex(dst, obj, idx)` | `R(dst) = R(obj)[R(idx)]` |
| `SetIndex(obj, idx, val)` | `R(obj)[R(idx)] = R(val)` |
| `Spread(dst, src)` | Spread `R(src)` into array being built at `R(dst)` |

### 4.10 Fields and Structs

| Instruction | Semantics |
|---|---|
| `GetField(dst, obj, u32)` | `R(dst) = R(obj).field[hash]` |
| `SetField(obj, u32, val)` | `R(obj).field[hash] = R(val)` |
| `MakeStruct(dst, u32, start, count)` | `R(dst) = Struct(name_hash, R(start)..R(start+count))` |
| `MakeClass(dst, u32, start, count)` | `R(dst) = Class(name_hash, R(start)..R(start+count))` |

### 4.11 Coroutines

| Instruction | Semantics |
|---|---|
| `StartCoroutine(base_reg, arg_count)` | Start coroutine, args in consecutive regs |
| `Yield` | Bare yield (suspend one frame) |
| `YieldValue(src)` | Yield with value |
| `YieldSeconds(src)` | Yield for seconds |
| `YieldFrames(src)` | Yield for frames |
| `YieldUntil(src)` | Yield until predicate |
| `YieldCoroutine(dst, src)` | `R(dst) = yield coroutine R(src)` |

### 4.12 Quickened Instructions

Runtime type specialization. Generic instructions rewrite themselves to quickened variants on first execution:

| Generic | Quickened (Int) | Quickened (Float) |
|---|---|---|
| `Add(d,a,b)` | `QAddInt(d,a,b)` | `QAddFloat(d,a,b)` |
| `Sub(d,a,b)` | `QSubInt(d,a,b)` | `QSubFloat(d,a,b)` |
| `Mul(d,a,b)` | `QMulInt(d,a,b)` | `QMulFloat(d,a,b)` |
| `Div(d,a,b)` | `QDivInt(d,a,b)` | `QDivFloat(d,a,b)` |
| `Lt(d,a,b)` | `QLtInt(d,a,b)` | `QLtFloat(d,a,b)` |
| ... | ... | ... |

Quickened variants try the typed fast path; on type mismatch they deopt back to generic.

---

## 5. CallFrame

```rust
pub(crate) struct CallFrame {
    pub chunk_id: ChunkId,
    pub pc: usize,
    pub base: usize,
    /// Absolute stack position where the caller wants the return value.
    pub result_reg: usize,
    /// Whether this was a dynamic Call (callee value was on stack).
    pub has_callee_slot: bool,
    pub upvalues: Option<Vec<Rc<RefCell<Value>>>>,
}
```

### 5.1 Return Mechanics

On `Return(src)`:
1. Read return value from `R(src)` via `cheap_clone()`.
2. Pop the frame.
3. Close upvalues if `has_open_upvalues`.
4. Truncate stack to `frame.base` (or use `unsafe set_len` for scalar-only frames).
5. Write return value to `stack[frame.result_reg]`.
6. Reload caller's IP, base, chunk_id.

---

## 6. Register Allocation

The compiler uses a linear stack-based allocator, not graph coloring.

```
struct RegisterAllocator {
    next_reg: u8,     // Next available register
    max_reg: u8,      // High-water mark → becomes max_registers
}
```

- **Locals:** allocated sequentially (`alloc_local() -> u8`)
- **Temporaries:** stack discipline (`alloc_temp() -> u8`, `free_temp(reg)`)
- **Destination propagation:** `compile_expr(expr, dst: Option<u8>) -> u8` — when the destination is known (e.g., `let x = expr`), pass it as hint to avoid a `Move`

### 6.1 Expression Compilation

`compile_expr` returns the register containing the result:
- **Identifier:** returns the local's register directly (no instruction emitted)
- **Literal:** emits `LoadInt(dst, value)`, returns `dst`
- **Binary:** compiles operands, emits three-address instruction, returns `dst`
- **Call:** places args in consecutive regs, emits `CallDirect`, returns `base_reg`

---

## 7. Eliminated Concepts

| Stack-Based | Register-Based |
|---|---|
| `Pop` instruction | Eliminated — temps freed at compile time |
| `LoadLocal(slot)` | Eliminated — identifiers resolve to register reference |
| `StoreLocal(slot)` | Eliminated — assignments use `Move` or destination propagation |
| `type_stack: Vec<ExprType>` | Replaced by `reg_types: Vec<ExprType>` |
| All 6 peephole fusion passes | Absorbed into native three-address emission |
| `IncrLocalInt` superinstruction | Now `AddIntImm(slot, slot, imm)` |
| `ReturnLocal` superinstruction | Now `Return(reg)` |
| `AddLocals` / `SubLocals` | Now `AddInt(dst, a, b)` / `SubInt(dst, a, b)` |
| `CmpLocalIntJump` / `CmpLocalsJump` | Now `TestLeInt` / `TestLeIntImm` etc. |

---

## 8. Performance Impact

| Bottleneck | Stack-Based Cost | Register-Based Cost |
|---|---|---|
| `drop_in_place<Value>` on Return | Run Drop on every local | Truncate or `set_len` (no per-value drop) |
| `Value::cheap_clone` on LoadLocal | Clone every time a local is read | Read by reference (zero clone) |
| `Vec::push/pop` on every instruction | 2-3 push/pops per arithmetic op | Zero push/pop for arithmetic |
| Stack length tracking | `self.stack.len()` on every op | Not needed (registers are indexed) |

Expected improvements: -15% to -40% depending on benchmark (largest on fibonacci at -30-40%).

---

## 9. Design Constraints

1. **No NaN boxing** — Values remain native Rust types.
2. **Minimal FFI marshaling** — Host functions continue to receive/return `Value` directly.
3. **Instruction pointer caching preserved** — Raw `*const Instruction` with pre-computed table.
4. **Quickening preserved** — Runtime specialization via in-place instruction rewrite.

---

## 10. Revision History

| Date | Change |
|---|---|
| 2026-03-05 | Initial draft |
