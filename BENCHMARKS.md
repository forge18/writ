# Writ Benchmarks

Writ includes a [criterion](https://github.com/bheisler/criterion.rs)-based benchmark suite for tracking VM and full-pipeline performance. The benchmarks are standard programs used across scripting language communities (Wren, Lua, AWFY) to enable cross-language comparison.

## Running Benchmarks

```sh
cargo bench --bench vm        # VM execution only (no lex/parse/compile)
cargo bench --bench pipeline  # Full pipeline (lex + parse + compile + execute)
```

## Benchmark Programs

| Benchmark | Description | Source |
|-----------|-------------|--------|
| **fibonacci(28)** | Recursive naive Fibonacci. Stresses function call overhead and arithmetic. | Wren, AWFY, CLBG |
| **binary_trees(8)** | Build and traverse deeply nested tree structures. Stresses object allocation. | Wren, CLBG, AWFY |
| **permute(9)** | Generate permutations via Heap's algorithm structure. Stresses recursion. | AWFY |
| **mandelbrot(100)** | Compute Mandelbrot set on a 100x100 grid. Stresses floating-point arithmetic and nested loops. | AWFY, CLBG |
| **sieve(5000)** | Sieve of Eratosthenes up to 5000. Stresses array subscript and index assignment. | Wren, AWFY, CLBG |
| **queens(8)** | N-Queens solver using closures. Stresses nested function capture (upvalues) and mutable shared state. | AWFY |
| **loop_sum(10k)** | Sum integers 0..10000 in a tight loop. Measures raw instruction dispatch throughput. | — |

## Results (Apple M-series, single-threaded)

| Benchmark | VM Only (v1 baseline) | VM Only (Round 1) | VM Only (Round 2) | VM Only (Round 3) | VM Only (Round 4) | VM Only (Round 5) | VM Only (Round 6) | VM Only (Round 7) | VM Only (Round 8) | Total Improvement |
|-----------|----------------------|-------------------|-------------------|-------------------|-------------------|-------------------|-------------------|-------------------|-------------------|-------------------|
| fibonacci_28 | 154 ms | 104 ms | 99 ms | 97 ms | 97 ms | 96 ms | 29.3 ms | 34.9 ms | 32.4 ms | -79% |
| binary_trees | 133 ms | 106 ms | 99 ms | 101 ms | 103 ms | 102 ms | 86.3 ms | 83.4 ms | 85.2 ms | -36% |
| permute_9 | 158 ms | 93 ms | 88 ms | 83 ms | 83 ms | 83 ms | 32.1 ms | 38.3 ms | 35.1 ms | -78% |
| mandelbrot_100 | 67 ms | 35.6 ms | 32.3 ms | 29.7 ms | 27.2 ms | 26.3 ms | 29.2 ms | 16.1 ms | 15.8 ms | -76% |
| sieve_5000 | 2.0 ms | 1.16 ms | 1.07 ms | 0.96 ms | 0.98 ms | 0.95 ms | 0.78 ms | 0.56 ms | 0.55 ms | -73% |
| queens_8 | 17.6 ms | 10.8 ms | 9.5 ms | 8.85 ms | 8.59 ms | 8.80 ms | 7.10 ms | 4.59 ms | 4.33 ms | -75% |
| loop_sum | 0.77 ms | 0.353 ms | 0.321 ms | 0.280 ms | 0.265 ms | 0.281 ms | 0.30 ms | 0.15 ms | 0.149 ms | -81% |

## Cross-Language Comparison

The most directly comparable benchmark is **fib(28)**, which Wren also uses as its standard fibonacci benchmark. Data for other languages comes from [Muxup's Wren benchmark update](https://muxup.com/2023q2/updating-wrens-benchmarks) (AMD Ryzen 9 5950X). Wren/Lua/Python/Ruby numbers are from running fib(28) x5; values below are divided by 5 for single-call comparison.

| Language | fib(28) | Notes |
|----------|---------|-------|
| LuaJIT 2.1 (-joff) | ~11 ms | Bytecode interpreter (JIT disabled) |
| Lua 5.4 | ~18 ms | Standard Lua VM |
| Ruby 3.0 | ~23 ms | YARV bytecode VM |
| Wren 0.4 | ~30 ms | NaN-boxed single-pass compiler |
| Python 3.11 | ~31 ms | Specializing adaptive interpreter |
| **Writ** | **32 ms** | Register-based VM (no NaN-boxing) |
| Rhai | ~225 ms | Rust-embeddable, AST-walking (estimated) |

### Key Observations

- **Writ is competitive with Wren/Python** on function-call-heavy benchmarks (~32ms vs ~30ms), down from ~5x slower after eight rounds of optimization. Round 8 recovered most of the fib/permute regression from the Round 7 register conversion via fast return paths and upvalue optimization.
- **Writ is faster than Rhai**, the most comparable Rust-embeddable scripting language. Rhai documents itself as "roughly 2x slower than Python 3" and uses AST-walking rather than bytecode compilation.
- **Different hardware** between our results (Apple M-series) and the Muxup results (AMD Ryzen 9 5950X). Ratios are more meaningful than absolute numbers.
- **Binary trees are not directly comparable** — Wren uses depth=12 with the CLBG structure; we use depth=8 with 100 iterations.

### Optimizations Applied

**Round 1** (154ms → 104ms, -32%):

1. **Release profile** — LTO (fat), codegen-units=1, panic=abort for cross-crate inlining.
2. **Value enum compaction** — Boxed `Closure` variant to shrink Value from ~40 to ~24 bytes.
3. **Hot loop overhead** — Cached chunk length, gated instruction counting, deferred PC write-back.
4. **Rc string pool** — `LoadStr` clones `Rc` (refcount bump) instead of allocating a new `String` + `Rc`.
5. **Direct function calls** — `CallDirect(func_idx, arity)` bypasses string-based function lookup.
6. **Constant pools** — 64-bit int/float literals stored in pools, referenced by u16 index.
7. **Upvalue guard** — Bool cache avoids `HashMap::is_empty()` on every `LoadLocal`.
8. **Peephole fusion** — `IncrLocalInt`, `CmpLocalIntJump`, `LoadLocalAddInt`, `LoadLocalSubInt`.

**Round 2** (104ms → 99ms, -5%):

1. **Selective PC writeback** — Removed per-instruction PC sync; only sync before error-producing instructions and Call/Yield.
2. **Upvalue guard on Return** — Skip `close_upvalues_above()` when no closures are active.
3. **Inlined i32 arithmetic** — Replaced function-pointer indirection in `AddInt`/`SubInt`/`MulInt`/`DivInt` with direct inline match.
4. **JumpIfFalsePop fusion** — Fused `JumpIfFalse + Pop` into single instruction for if/while/for/when.
5. **Shrunk CallFrame** — Removed redundant `func_index` and `truncate_to` fields (80 → 64 bytes).
6. **Cached instruction pointer** — Raw `*const Instruction` pointer eliminates `chunk_for()` dispatch and bounds check per instruction.
7. **Stack/frame pre-allocation** — `Vec::with_capacity(256)` for stack, `Vec::with_capacity(64)` for frames.

**Round 3** (99ms → 97ms, -2% fib; up to -13% on arithmetic-heavy benchmarks):

1. **Unsafe stack operations** — Replaced bounds-checked `Vec` indexing with `get_unchecked` / `get_unchecked_mut` for all 20 typed arithmetic/comparison handlers and 4 fused instructions. Used `set_len` instead of `truncate` where dropped values are Copy-like (Int/Float/Bool).
2. **Auto-advancing instruction pointer** — Changed from `*ip.add(pc); pc += 1` to `*ip; ip = ip.add(1)`. Jump instructions use pointer offset arithmetic. Integer PC derived from pointer only when needed (error paths, frame save).
3. **Call/Return fast path** — `unwrap_unchecked` on frame/stack pops, skip truncation when stack is already at target length, pointer-based jump in `JumpIfFalsePop`.

**Round 4** (29.7ms → 27.2ms mandelbrot, -8%; up to -6% on loop-heavy benchmarks):

1. **Quickening** — Runtime bytecode specialization for generic arithmetic and comparison instructions. On first execution, generic `Add`/`Sub`/`Mul`/`Div`/`Lt`/`Le`/`Gt`/`Ge`/`Eq`/`Ne` are rewritten in-place to quickened variants (`QAddInt`, `QAddFloat`, etc.) that try a typed fast path and deopt back to generic on type mismatch. Eliminates helper function call overhead for type-stable code paths.
2. **Instruction pointer cache** — Pre-computed `(*const Instruction, len)` table for all function chunks, populated at load time. Eliminates the `chunk_for().instructions().as_ptr()` chain on every `Call`/`Return`/frame reload. Reduces Call/Return overhead by ~3%.
3. **Computed goto investigation** — Verified via `cargo-show-asm` that LLVM already generates a jump table (`LJTI`) for the `match instruction` dispatch. No further dispatch optimization needed.

**Round 5** (mixed: -3% mandelbrot, -4% sieve; ~0% fib/permute; +2% queens/loop_sum):

1. **Value enum flattening** — Removed nested `IntValue` (`I32`/`I64`) and `FloatValue` (`F32`/`F64`) sub-enums. All four numeric variants (`I32`, `I64`, `F32`, `F64`) are now direct `Value` variants. Eliminates one layer of discriminant checking on arithmetic paths.
2. **Canonical hash consistency** — Fixed `Hash` impl to use type tags (0 for all ints, 1 for all floats) so cross-width equal values (`I32(42) == I64(42)`) hash identically.
3. **Hot-reload IP cache invalidation** — Fixed stale instruction pointer cache after `VM::reload()`, which caused SIGSEGV on reloaded functions.
4. **LSP completeness** — Added `ExprKind::Index` handling to references and rename traversals.

**Analysis:** The 13-variant flat enum produces slightly worse branch prediction for tight integer loops (loop_sum, queens) compared to the 7+2 nested approach. However, it eliminates the nested match overhead for arithmetic operations and simplifies the codebase. The `Object(Rc<RefCell<dyn WritObject>>)` fat pointer still forces Value to 24 bytes; boxing it to 16 bytes was tested but regressed object-heavy benchmarks (binary_trees +11%) due to extra indirection cost.

**Round 6** (96ms → 29ms fib, -69%; 83ms → 32ms permute, -61%; broad improvements across suite):

1. **Superinstructions** — Added `ReturnLocal(slot)`, `AddLocals(a, b)`, `SubLocals(a, b)`, and `CmpLocalsJump(a, b, op, offset)` fused instructions with corresponding peephole passes and VM handlers.
2. **Peephole jump fixup infrastructure** — Implemented `fixup_jumps()` to correctly adjust all jump offsets after instruction removal during peephole fusion. Previous approach had no fixup, causing jump corruption in multi-pass optimization.
3. **Recursive CallDirect** — Pre-register function index before body compilation so recursive self-calls emit `CallDirect(func_idx, arity)` instead of `LoadGlobal + Call`. Gated on top-level functions only to preserve closure semantics.
4. **Return type tracking** — Store declared return types (`function_return_types` map) so that CallDirect call sites push the correct `ExprType`, enabling typed instruction emission (e.g., `AddInt` instead of generic `Add`) for expressions involving function return values.
5. **Parameter and local type tracking** — Added `type_tag` field to `Local` struct, populated from parameter type annotations and initializer expression types. `resolve_local()` now returns `(slot, type_tag)`, enabling typed instruction emission for all local variable operations.
6. **ReturnLocal upvalue correctness** — `ReturnLocal` handler checks `open_upvalues` HashMap before reading stack slot, matching `LoadLocal` semantics for captured variables.

**Analysis:** The dominant win comes from items 3-5: recursive functions like `fib` and `permute` previously used `LoadGlobal + Call` (string-based lookup) and generic arithmetic (Add/Sub/Le). After this round, `fib(28)` compiles to 11 instructions (down from 17) using `CallDirect`, `LeInt`, `AddInt`, `LoadLocalSubInt`, and `ReturnLocal`. The combination of eliminating global lookup, typed dispatch, and superinstructions produced a 3.3x speedup on call-heavy benchmarks. Mandelbrot and loop_sum show slight regressions from added branches in the larger instruction dispatch table.

**Round 7** (16ms mandelbrot, -45%; 0.15ms loop_sum, -50%; 4.6ms queens, -35%):

1. **Register-based VM (Lua 5.x model)** — Complete rewrite from stack-based to register-based execution. Registers are frame-relative slots in the existing `Vec<Value>`. Each function frame owns `[base..base+max_registers)`. Instructions use three-address format: `AddInt(dst, src_a, src_b)` where all operands are `u8` register indices. Eliminates all `push`/`pop`/`cheap_clone`/`drop_in_place` from arithmetic hot paths.
2. **Register allocator in compiler** — Linear stack-based allocator with destination propagation. `compile_expr` returns the register containing the result and accepts an optional destination hint. Locals resolve directly to their register — no `LoadLocal`/`StoreLocal` emission needed.
3. **Three-address instruction set** — Replaced entire `Instruction` enum. Old stack instructions (`Push`, `Pop`, `LoadLocal`, `StoreLocal`, `AddLocals`, `ReturnLocal`, etc.) replaced by register variants (`Move`, `AddInt(d,a,b)`, `Return(src)`, `CallDirect(base_reg, func_idx, arity)`). Round 6 superinstructions absorbed into native three-address format.
4. **Quickening preserved** — Generic `Add(d,a,b)` rewrites to `QAddInt(d,a,b)` or `QAddFloat(d,a,b)` at runtime. Deopt path reverts to generic and falls through to type dispatch.
5. **Fused test-and-branch** — `TestLtInt(a, b, offset)`, `TestLtIntImm(a, imm, offset)`, and float variants fuse comparison + conditional jump into single instructions. Eliminates temporary register and separate `JumpIfFalsy`.
6. **Open upvalue write-through** — `StoreUpvalue` syncs values back to parent stack slots via `open_upvalues` HashMap, so parent functions see mutations to captured variables through direct register reads. `MakeClosure` syncs self-referential captures for recursive closures.

**Analysis:** The register conversion directly addressed the #1 profiling bottleneck (`drop_in_place<Value>` at 10-38%). Arithmetic instructions now read source registers by reference and write results in-place — no `push`/`pop`/`clone`/`drop` cycle. The biggest wins are on tight arithmetic loops: mandelbrot (-45%), loop_sum (-50%), sieve (-28%), queens (-35%). Fibonacci and permute show slight regressions (+19%, +19%) because the register calling convention has higher per-call overhead than the old stack model (extend stack to `max_registers`, write result to `result_reg`, truncate). These benchmarks are dominated by call overhead rather than arithmetic. Binary trees shows a small improvement (-3%) as expected — it's dominated by object allocation, not stack ops.

**Round 8** (32ms fib, -7%; 35ms permute, -8%; 4.3ms queens, -6%):

1. **Fast return path** — Added `has_rc_values: bool` to `CompiledFunction` and `CallFrame`. Compiler determines at compile time whether any register could hold an Rc-bearing value (Str, Array, Dict, Object, Closure). Return handler uses `unsafe { set_len }` instead of `truncate` for scalar-only frames, skipping `drop_in_place` on every value in the frame window.
2. **Upvalue Vec replacement** — Replaced `open_upvalues: HashMap<usize, Rc<RefCell<Value>>>` with `Vec<Option<Rc<RefCell<Value>>>>` indexed by absolute stack slot. Eliminates SipHash overhead on every `capture_local`, `close_upvalues_above`, and upvalue write-through operation. `close_upvalues_above` now iterates a contiguous slice instead of calling `HashMap::retain`.

**Analysis:** The fast return path directly addresses the remaining `drop_in_place` overhead from Round 7. Functions like `fib` and `permute` use only Int registers, so every return now skips all drop glue via `set_len` — a single pointer write instead of per-element destructor calls. The upvalue Vec replacement eliminates hashing overhead that was 12% of queens execution time. Together these recover most of the fib/permute regression (+19% → +10% from Round 6 baseline) while further improving queens (-6%) and keeping arithmetic benchmarks stable.

## Profiling Analysis (Post-Round 6)

Flamegraph profiling (`cargo flamegraph`) of all 7 benchmarks reveals the dominant cost centers. Flamegraph SVGs are in `flamegraphs/`.

### Top Bottlenecks by Sample Weight

| Bottleneck | fibonacci | permute | binary_trees | mandelbrot | sieve | queens | loop_sum |
| ---------- | --------- | ------- | ----------- | ---------- | ----- | ------ | -------- |
| `drop_in_place<Value>` (stack cleanup) | **38%** | **13%** | — | **12%** | 6% | 7% | **17%** |
| `Value::cheap_clone` / extraction | 2% | 1% | — | **10%** | **22%** | — | 3% |
| Object alloc/dealloc (HashMap) | — | — | **50%** | — | — | — | — |
| SipHash (`open_upvalues` HashMap) | — | — | — | — | 4% | **12%** | — |
| `promote_float_pair_op` | — | — | — | **6%** | — | — | — |
| `exec_call` / frame overhead | — | — | — | — | — | 5% | — |
| `Vec::push` / `Vec::pop` (stack ops) | 1% | 4% | — | 2% | — | — | 1% |

### Key Findings

1. **Stack drop glue is the #1 cost** — `Value` is a 24-byte enum with non-Copy variants (`Str`, `Object` contain `Rc`). Every `Return` truncates the stack, running `drop_in_place` on each value even when most are I32/Bool that need no dropping. This dominates fibonacci (38%) and is significant everywhere.

2. **Object representation is catastrophic for binary_trees** — Each class instance is a `HashMap<String, Value>` inside `Rc<RefCell<dyn WritObject>>`. Creating and destroying tree nodes spends ~50% of time in HashMap alloc/dealloc/drop. A fixed-layout struct representation would eliminate this entirely.

3. **Upvalue HashMap is expensive for closures** — Queens spends 12% in `SipHasher13::write` from `open_upvalues: HashMap<usize, Rc<RefCell<Value>>>` lookups. Upvalue slot indices are small integers — a direct-indexed `Vec` or fixed-size array would be O(1) with no hashing.

4. **Float promotion overhead** — Mandelbrot spends 6% in `promote_float_pair_op` for mixed-type arithmetic. The compiler should emit `MulFloat`/`AddFloat` directly for float-typed expressions; current type tracking covers locals but not all intermediate sub-expressions.

5. **Value extraction (`as_i64`, `as_f64`, `cheap_clone`)** — Every arithmetic instruction pops values, matches discriminants, extracts the inner type, computes, and pushes a new Value. A register-based VM would keep values in place, eliminating most clone/extract/push cycles.

## Remaining Optimization Opportunities

Ordered by profiling-informed expected impact:

1. **Compact object representation** (est. -30-40% binary_trees) — Replace per-instance `HashMap<String, Value>` with fixed-layout field arrays. Field offsets resolved at compile time. Eliminates HashMap alloc/dealloc/hashing for struct/class instances.

2. **Call/Return frame optimization** (est. -5-15% fib/permute) — Inline frame stack (`[CallFrame; 64]`), split cold upvalue fields to side-table, frame reuse for CallDirect. Remaining bottleneck for call-heavy benchmarks after fast return path eliminated most drop glue.

3. **Float type propagation** (est. -5-10% mandelbrot) — Extend compiler type tracking to propagate float types through sub-expressions (`a * b` where both are float → emit `MulFloat` directly). Currently only locals and function returns carry type info.

4. **Instruction encoding compaction** (est. -3-5%) — Encode instructions as compact `u32` words (8-bit opcode + 24-bit operands) instead of Rust enum (~16 bytes). 4x better instruction cache density.

5. **Tail call optimization** — `CallDirect + Return` → `TailCallDirect` that reuses the current frame. Prevents stack overflow on tail-recursive code but does not help current benchmarks (fib/permute are not tail-recursive).

## Design Constraints

1. **No NaN boxing** — Values remain native Rust data types (`i32`/`i64`, `f32`/`f64`, `bool`, `Rc<String>`, etc.). This preserves zero-cost FFI with Rust host code: no marshaling, no conversion, no boxing/unboxing at the boundary.
2. **Minimal FFI marshaling** — Writ is designed for embedding in Rust applications. Host-registered functions receive and return `Value` directly. Any optimization must preserve this property — Writ values ARE Rust values.

## References

- [Wren Performance](https://wren.io/performance.html)
- [Updating Wren's Benchmarks (Muxup)](https://muxup.com/2023q2/updating-wrens-benchmarks)
- [Rhai Benchmarks](https://rhai.rs/book/about/benchmarks.html)
- [Are We Fast Yet (AWFY)](https://github.com/smarr/are-we-fast-yet)
- [Computer Language Benchmarks Game (CLBG)](https://benchmarksgame-team.pages.debian.net/benchmarksgame/)
