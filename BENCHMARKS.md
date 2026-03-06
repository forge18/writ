# Writ Benchmarks

Writ includes a [criterion](https://github.com/bheisler/criterion.rs)-based benchmark suite for tracking VM and full-pipeline performance. The benchmarks are standard programs used across scripting language communities (Wren, Lua, AWFY) to enable cross-language comparison.

## Running Benchmarks

```sh
cargo bench --bench vm        # VM execution only (no lex/parse/compile)
cargo bench --bench pipeline  # Full pipeline (lex + parse + compile + execute)
cargo bench --bench compiler  # Compiler only (parse → bytecode)
cargo bench --bench lexer     # Lexer only (source → tokens)
cargo bench --bench parser    # Parser only (tokens → AST)
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

| Benchmark | VM Only (v1 baseline) | VM Only (Round 1) | VM Only (Round 2) | VM Only (Round 3) | VM Only (Round 4) | VM Only (Round 5) | VM Only (Round 6) | VM Only (Round 7) | VM Only (Round 8) | VM Only (Round 9) | VM Only (Round 10) | VM Only (Round 11) | VM Only (Round 12) | Total Improvement |
| --------- | -------------------- | ----------------- | ----------------- | ----------------- | ----------------- | ----------------- | ----------------- | ----------------- | ----------------- | ----------------- | ------------------ | ------------------ | ------------------ | ----------------- |
| fibonacci_28 | 154 ms | 104 ms | 99 ms | 97 ms | 97 ms | 96 ms | 29.3 ms | 34.9 ms | 32.4 ms | 31.6 ms | 32.5 ms | 31.6 ms | 31.8 ms | -79% |
| binary_trees | 133 ms | 106 ms | 99 ms | 101 ms | 103 ms | 102 ms | 86.3 ms | 83.4 ms | 85.2 ms | 83.1 ms | 38.9 ms | 37.4 ms | 37.7 ms | -72% |
| permute_9 | 158 ms | 93 ms | 88 ms | 83 ms | 83 ms | 83 ms | 32.1 ms | 38.3 ms | 35.1 ms | 34.7 ms | 35.0 ms | 34.5 ms | 34.2 ms | -78% |
| mandelbrot_100 | 67 ms | 35.6 ms | 32.3 ms | 29.7 ms | 27.2 ms | 26.3 ms | 29.2 ms | 16.1 ms | 15.8 ms | 15.3 ms | 16.6 ms | 15.8 ms | 15.1 ms | -77% |
| sieve_5000 | 2.0 ms | 1.16 ms | 1.07 ms | 0.96 ms | 0.98 ms | 0.95 ms | 0.78 ms | 0.56 ms | 0.55 ms | 0.536 ms | 0.563 ms | 0.546 ms | 0.543 ms | -73% |
| queens_8 | 17.6 ms | 10.8 ms | 9.5 ms | 8.85 ms | 8.59 ms | 8.80 ms | 7.10 ms | 4.59 ms | 4.33 ms | 4.28 ms | 4.35 ms | 4.25 ms | 4.20 ms | -76% |
| loop_sum | 0.77 ms | 0.353 ms | 0.321 ms | 0.280 ms | 0.265 ms | 0.281 ms | 0.30 ms | 0.15 ms | 0.149 ms | 0.146 ms | 0.159 ms | 0.153 ms | 0.148 ms | -81% |

## Compilation Pipeline Results (Round 9)

| Benchmark | Lexer | Parser | Compiler | Pipeline (full) |
| --------- | ----- | ------ | -------- | --------------- |
| fibonacci | 885 ns | 1.48 µs | 765 ns | 31.8 ms |
| structs | 1.02 µs | 1.43 µs | 1.32 µs | — |
| loop | — | 817 ns | 388 ns | — |
| arithmetic | 216 ns | — | — | — |
| interpolation | 668 ns | — | — | — |

Pipeline numbers include lex + parse + compile + VM execution. Compiler and lexer are measured in isolation on the same source programs.

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

- **Writ is competitive with Wren/Python** on function-call-heavy benchmarks (~32ms vs ~30ms), down from ~5x slower after twelve rounds of optimization. Round 10's compact object representation brought binary_trees from -38% to -72% total improvement.
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

**Round 9** (31.6ms fib, -2%; 15.3ms mandelbrot, -3%; 0.536ms sieve, -2%; broad minor improvements):

1. **Typed native binding layer** — Replaced `NativeFn = Rc<dyn Fn(&[Value]) -> Result<Value, String>>` with `FromValue`/`IntoValue`/`IntoNativeHandler` traits in `binding.rs`. Native functions are now registered with typed signatures (`fn1(|x: f64| -> Result<f64, String> { Ok(x.sqrt()) })`); the binding layer generates monomorphized extraction code at registration time.
2. **Stack-slice dispatch** — `exec_native_call_reg` passes `&self.stack[arg_start..arg_start + n]` directly to the handler instead of allocating a `Vec<Value>` per call. Eliminates one heap allocation and `n` `cheap_clone` calls per native invocation.
3. **Arity inference** — `IntoNativeHandler::arity()` is a static method returning `Option<u8>`, so native function arity is encoded at registration time from the closure signature — no manually-passed arity argument.
4. **Width coercion at the boundary** — `FromValue for i64` widens `I32→i64` for free (`*n as i64`); `FromValue for i32` range-checks `I64→i32` via `i32::try_from`. Spec-correct narrowing errors propagate as `RuntimeError` with the argument position in the message.

**Analysis:** The dominant win is eliminating the per-call `Vec<Value>` allocation. Each stdlib call previously heap-allocated a Vec, cloned `n` args into it, passed a `&[Value]` slice, then dropped the Vec. With stack-slice dispatch, the allocation is gone — the handler reads directly from the register window. Improvements are modest across benchmarks because stdlib calls are not on most hot paths (fibonacci/permute/loop_sum are pure arithmetic). The largest gains appear on sieve and mandelbrot, which exercise arithmetic helpers. The typing machinery compiles away entirely — `FromValue` extractors are inlined by LLVM into direct match-on-discriminant sequences identical to the old hand-written pattern matching, with no overhead added.

**Round 10** (38.9ms binary_trees, -53%; all other benchmarks within noise):

1. **Compact object representation** — Replaced per-instance `HashMap<String, Value>` field storage in `WritStruct` and `WritClassInstance` with `Vec<Value>` backed by shared `Rc<FieldLayout>`. The `FieldLayout` struct (type name, field count, `hash_to_index: HashMap<u32, usize>`, field names, public fields/methods) is built once per type at load time and shared across all instances via `Rc`.
2. **Eliminated per-construction overhead** — `exec_make_struct_reg` now does `Rc::clone(&layout)` + `Vec::with_capacity` + push Values instead of N string clones + N HashMap inserts + Vec/HashSet clones. No string operations in the construction hot path.
3. **Single-lookup field access** — `get_field_by_hash(u32)` and `set_field_by_hash(u32, Value)` methods use the instruction's pre-computed FNV-1a hash to index directly into the Vec via the shared layout's `hash_to_index` map. One small HashMap lookup (3 entries for binary_trees) instead of two full HashMap lookups.
4. **WritStruct size reduction** — `WritStruct` shrank from ~192 bytes (String + HashMap + Vec + 2×HashSet) to ~32 bytes (Rc + Vec). `Box<WritStruct>` in the `Value` enum remains 8 bytes.

**Analysis:** binary_trees was the weakest benchmark at -38% total improvement, with profiling showing ~50% of time in HashMap alloc/dealloc/drop for object instances. The compact representation eliminates all per-instance HashMap operations — construction, field access, and destruction are now Vec-based with shared metadata. The -53% improvement on binary_trees (83.1ms → 38.9ms) exceeds the estimated -30-40%. Other benchmarks show no change (within noise threshold) as expected — they don't use struct/class instances on their hot paths.

**Round 11** (15.8ms mandelbrot, -5%; 0.153ms loop_sum, -4%; 0.546ms sieve, -3%; broad 1-4% improvements):

1. **VM struct hot-field layout** — Added `#[repr(C)]` to pin field order and grouped the dispatch loop's most-accessed fields onto the first two 64-byte cache lines. Cache line 0 (offsets 0–57): `stack`, `frames`, `instruction_count`, `has_debug_hooks`, `has_open_upvalues`. Cache line 1 (offsets 64–127): `func_ip_cache`, `functions`, `instruction_limit`. Cold fields (HashMaps, debug hooks, coroutines) pushed to cache lines 3+.

**Analysis:** Without `#[repr(C)]`, Rust reorders struct fields for alignment optimization, scattering hot fields across distant cache lines. Profiling with `offset_of!` showed the previous auto-layout placed `instruction_count` at offset 984 (cache line 15) and `has_debug_hooks` at offset 1068 (cache line 16), even though both are checked every iteration of the dispatch loop. The `stack` (offset 48) and `frames` (offset 72) headers also straddled a cache line boundary. Pinning all per-instruction fields to CL 0 and call/return fields to CL 1 reduced cache misses uniformly across all benchmarks. A companion attempt to compact `CallFrame` from 72 to 20 bytes (eliminating `ChunkId` enum, moving upvalues to a side table, narrowing fields to `u32`) was reverted — while structurally sound, it perturbed LLVM's code generation for the 52KB `run_until` function, causing uniform 5-9% regressions on non-call benchmarks that outweighed the frame-size benefits.

**Round 12** (15.1ms mandelbrot, -5%; 0.148ms loop_sum, -3%; broad minor improvements):

1. **Float type propagation** — Added `IntToFloat(dst, src)` instruction that widens I32/I64 → F64 at compile time. The compiler now detects mixed int/float binary operations (e.g., `x * 1.0` where `x` is int) and emits an `IntToFloat` coercion followed by typed float instructions (`MulFloat`, `AddFloat`, etc.) instead of falling through to generic dispatch. This eliminates runtime `promote_float_pair_op` overhead for statically-typed expressions.
2. **Self-recursive tail call optimization** — Added `TailCallDirect(base, func_idx, arity)` instruction. Peephole pass detects `CallDirect(base, self_idx, arity) + Return(base)` patterns where the callee is the same function (self-recursive), and fuses them into a single `TailCallDirect` that reuses the current frame instead of pushing/popping. Restricted to self-recursive calls to preserve cross-function stack traces. Prevents stack overflow for tail-recursive user programs.

**Analysis:** The float type propagation directly addresses the mandelbrot hotspot: `promote_float_pair_op` consumed ~6% of execution time doing runtime float type dispatch for operations the compiler could resolve statically. The mandelbrot benchmark's inner loop has expressions like `2.0 * (x * 1.0) / fsize - 1.5` where `x` is int — previously the `x * 1.0` mixed operation emitted generic `Mul`, losing type info for the entire chain. With `IntToFloat` coercion, the compiler now emits `IntToFloat + MulFloat` and all subsequent operations stay typed. The tail call optimization has no benchmark impact (none of the benchmarks use self-recursive tail calls) but is a correctness improvement for user programs.

## Profiling Analysis (Post-Round 6)

Flamegraph profiling (`cargo flamegraph`) of all 7 benchmarks reveals the dominant cost centers. Flamegraph SVGs are in `flamegraphs/`.

### Top Bottlenecks by Sample Weight

| Bottleneck | fibonacci | permute | binary_trees | mandelbrot | sieve | queens | loop_sum |
| ---------- | --------- | ------- | ----------- | ---------- | ----- | ------ | -------- |
| `drop_in_place<Value>` (stack cleanup) | **38%** | **13%** | — | **12%** | 6% | 7% | **17%** |
| `Value::cheap_clone` / extraction | 2% | 1% | — | **10%** | **22%** | — | 3% |
| ~~Object alloc/dealloc (HashMap)~~ | — | — | ~~**50%**~~ | — | — | — | — |
| SipHash (`open_upvalues` HashMap) | — | — | — | — | 4% | **12%** | — |
| `promote_float_pair_op` | — | — | — | **6%** | — | — | — |
| `exec_call` / frame overhead | — | — | — | — | — | 5% | — |
| `Vec::push` / `Vec::pop` (stack ops) | 1% | 4% | — | 2% | — | — | 1% |

### Key Findings

1. **Stack drop glue is the #1 cost** — `Value` is a 24-byte enum with non-Copy variants (`Str`, `Object` contain `Rc`). Every `Return` truncates the stack, running `drop_in_place` on each value even when most are I32/Bool that need no dropping. This dominates fibonacci (38%) and is significant everywhere.

2. ~~**Object representation is catastrophic for binary_trees**~~ — **Fixed in Round 10.** Replaced per-instance `HashMap<String, Value>` with `Vec<Value>` + shared `Rc<FieldLayout>`. binary_trees improved -53% (83.1ms → 38.9ms).

3. **Upvalue HashMap is expensive for closures** — Queens spends 12% in `SipHasher13::write` from `open_upvalues: HashMap<usize, Rc<RefCell<Value>>>` lookups. Upvalue slot indices are small integers — a direct-indexed `Vec` or fixed-size array would be O(1) with no hashing.

4. **Float promotion overhead** — Mandelbrot spends 6% in `promote_float_pair_op` for mixed-type arithmetic. The compiler should emit `MulFloat`/`AddFloat` directly for float-typed expressions; current type tracking covers locals but not all intermediate sub-expressions.

5. **Value extraction (`as_i64`, `as_f64`, `cheap_clone`)** — Every arithmetic instruction pops values, matches discriminants, extracts the inner type, computes, and pushes a new Value. A register-based VM would keep values in place, eliminating most clone/extract/push cycles.

## Remaining Optimization Opportunities

1. **Instruction encoding compaction** (est. -3-5%) — Encode instructions as compact `u32` words (8-bit opcode + 24-bit operands) instead of Rust enum (~12 bytes). 3x better instruction cache density. High risk of LLVM codegen regression (as demonstrated by Round 11's compact CallFrame attempt).

## Design Constraints

1. **No NaN boxing** — Values remain native Rust data types (`i32`/`i64`, `f32`/`f64`, `bool`, `Rc<String>`, etc.). This preserves zero-cost FFI with Rust host code: no marshaling, no conversion, no boxing/unboxing at the boundary.
2. **Minimal FFI marshaling** — Writ is designed for embedding in Rust applications. Host-registered functions receive and return `Value` directly. Any optimization must preserve this property — Writ values ARE Rust values.
3. **Typed binding layer compatibility** — The typed native binding layer (`FromValue`/`IntoValue`/`IntoNativeHandler` in `binding.rs`) passes stack slices directly to native functions. All VM optimizations must preserve the register-window layout so `&self.stack[arg_start..arg_start+n]` remains valid. Native calls execute synchronously within the caller's frame (no `CallFrame` push/pop), so frame stack and return path optimizations do not affect the FFI boundary.

## References

- [Wren Performance](https://wren.io/performance.html)
- [Updating Wren's Benchmarks (Muxup)](https://muxup.com/2023q2/updating-wrens-benchmarks)
- [Rhai Benchmarks](https://rhai.rs/book/about/benchmarks.html)
- [Are We Fast Yet (AWFY)](https://github.com/smarr/are-we-fast-yet)
- [Computer Language Benchmarks Game (CLBG)](https://benchmarksgame-team.pages.debian.net/benchmarksgame/)
