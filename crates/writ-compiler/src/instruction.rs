/// Comparison operator encoding for fused compare-and-jump instructions.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum CmpOp {
    Lt = 0,
    Le = 1,
    Gt = 2,
    Ge = 3,
}

impl CmpOp {
    /// Converts a u8 tag back to a CmpOp.
    #[inline(always)]
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => CmpOp::Lt,
            1 => CmpOp::Le,
            2 => CmpOp::Gt,
            3 => CmpOp::Ge,
            _ => unreachable!(),
        }
    }
}

/// A single bytecode instruction for the Writ register-based VM.
///
/// Instructions use a three-address format: operands are register indices
/// (`u8`, frame-relative offsets from `CallFrame::base`). Values are read
/// from and written to registers in-place, eliminating the push/pop/clone/drop
/// overhead of a stack-based architecture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Instruction {
    // ── Load/Store ──────────────────────────────────────────────────
    /// `R(dst) = I32(imm)`
    LoadInt(u8, i32),
    /// `R(dst) = I64(pool[idx])`
    LoadConstInt(u8, u16),
    /// `R(dst) = F32(imm)`
    LoadFloat(u8, f32),
    /// `R(dst) = F64(pool[idx])`
    LoadConstFloat(u8, u16),
    /// `R(dst) = Bool(imm)`
    LoadBool(u8, bool),
    /// `R(dst) = Str(Rc::clone(pool[idx]))`
    LoadStr(u8, u32),
    /// `R(dst) = Null`
    LoadNull(u8),
    /// `R(dst) = R(src).cheap_clone()`
    Move(u8, u8),
    /// `R(dst) = globals[hash]`
    LoadGlobal(u8, u32),

    // ── Arithmetic (generic, quickenable) ───────────────────────────
    /// `R(dst) = R(a) + R(b)` — generic, quickened on first execution
    Add(u8, u8, u8),
    /// `R(dst) = R(a) - R(b)` — generic, quickened on first execution
    Sub(u8, u8, u8),
    /// `R(dst) = R(a) * R(b)` — generic, quickened on first execution
    Mul(u8, u8, u8),
    /// `R(dst) = R(a) / R(b)` — generic, quickened on first execution
    Div(u8, u8, u8),
    /// `R(dst) = R(a) % R(b)`
    Mod(u8, u8, u8),

    // ── Arithmetic (typed, compiler-guaranteed) ─────────────────────
    /// `R(dst) = R(a) +_int R(b)` — both operands known int
    AddInt(u8, u8, u8),
    /// `R(dst) = R(a) +_float R(b)` — both operands known float
    AddFloat(u8, u8, u8),
    /// `R(dst) = R(a) -_int R(b)`
    SubInt(u8, u8, u8),
    /// `R(dst) = R(a) -_float R(b)`
    SubFloat(u8, u8, u8),
    /// `R(dst) = R(a) *_int R(b)`
    MulInt(u8, u8, u8),
    /// `R(dst) = R(a) *_float R(b)`
    MulFloat(u8, u8, u8),
    /// `R(dst) = R(a) /_int R(b)`
    DivInt(u8, u8, u8),
    /// `R(dst) = R(a) /_float R(b)`
    DivFloat(u8, u8, u8),

    // ── Arithmetic (immediate forms) ────────────────────────────────
    /// `R(dst) = R(src) + I32(imm)` — fused load+add for increments
    AddIntImm(u8, u8, i32),
    /// `R(dst) = R(src) - I32(imm)` — fused load+sub
    SubIntImm(u8, u8, i32),

    // ── Unary ───────────────────────────────────────────────────────
    /// `R(dst) = -R(src)`
    Neg(u8, u8),
    /// `R(dst) = !R(src)`
    Not(u8, u8),

    // ── Comparison (generic, quickenable) ───────────────────────────
    /// `R(dst) = R(a) == R(b)` — generic, quickened on first execution
    Eq(u8, u8, u8),
    /// `R(dst) = R(a) != R(b)` — generic
    Ne(u8, u8, u8),
    /// `R(dst) = R(a) < R(b)` — generic
    Lt(u8, u8, u8),
    /// `R(dst) = R(a) <= R(b)` — generic
    Le(u8, u8, u8),
    /// `R(dst) = R(a) > R(b)` — generic
    Gt(u8, u8, u8),
    /// `R(dst) = R(a) >= R(b)` — generic
    Ge(u8, u8, u8),

    // ── Comparison (typed, compiler-guaranteed) ─────────────────────
    EqInt(u8, u8, u8),
    EqFloat(u8, u8, u8),
    NeInt(u8, u8, u8),
    NeFloat(u8, u8, u8),
    LtInt(u8, u8, u8),
    LtFloat(u8, u8, u8),
    LeInt(u8, u8, u8),
    LeFloat(u8, u8, u8),
    GtInt(u8, u8, u8),
    GtFloat(u8, u8, u8),
    GeInt(u8, u8, u8),
    GeFloat(u8, u8, u8),

    // ── Logical ─────────────────────────────────────────────────────
    /// `R(dst) = R(a) && R(b)` — non-short-circuit
    And(u8, u8, u8),
    /// `R(dst) = R(a) || R(b)` — non-short-circuit
    Or(u8, u8, u8),

    // ── Control flow ────────────────────────────────────────────────
    /// Unconditional relative jump. Offset from the NEXT instruction.
    Jump(i32),
    /// Jump if `R(src)` is falsy. Offset from the NEXT instruction.
    JumpIfFalsy(u8, i32),
    /// Jump if `R(src)` is truthy. Offset from the NEXT instruction.
    JumpIfTruthy(u8, i32),

    // ── Fused compare-and-jump (int) ────────────────────────────────
    // These test a condition and jump if the condition is FALSE (fall through = true).
    /// Jump if NOT `R(a) < R(b)` (int)
    TestLtInt(u8, u8, i32),
    /// Jump if NOT `R(a) <= R(b)` (int)
    TestLeInt(u8, u8, i32),
    /// Jump if NOT `R(a) > R(b)` (int)
    TestGtInt(u8, u8, i32),
    /// Jump if NOT `R(a) >= R(b)` (int)
    TestGeInt(u8, u8, i32),
    /// Jump if NOT `R(a) == R(b)` (int)
    TestEqInt(u8, u8, i32),
    /// Jump if NOT `R(a) != R(b)` (int)
    TestNeInt(u8, u8, i32),

    // ── Fused compare-and-jump (int immediate) ──────────────────────
    /// Jump if NOT `R(a) < imm` (int)
    TestLtIntImm(u8, i32, i32),
    /// Jump if NOT `R(a) <= imm` (int)
    TestLeIntImm(u8, i32, i32),
    /// Jump if NOT `R(a) > imm` (int)
    TestGtIntImm(u8, i32, i32),
    /// Jump if NOT `R(a) >= imm` (int)
    TestGeIntImm(u8, i32, i32),

    // ── Fused compare-and-jump (float) ──────────────────────────────
    /// Jump if NOT `R(a) < R(b)` (float)
    TestLtFloat(u8, u8, i32),
    /// Jump if NOT `R(a) <= R(b)` (float)
    TestLeFloat(u8, u8, i32),
    /// Jump if NOT `R(a) > R(b)` (float)
    TestGtFloat(u8, u8, i32),
    /// Jump if NOT `R(a) >= R(b)` (float)
    TestGeFloat(u8, u8, i32),

    // ── Return ──────────────────────────────────────────────────────
    /// Return `R(src)` to the caller's result register.
    Return(u8),
    /// Return Null to the caller's result register.
    ReturnNull,

    // ── Function calls ──────────────────────────────────────────────
    /// Dynamic call. `R(base)` is the callee value, args in `R(base+1)..R(base+1+arity)`.
    /// Result written to `R(base)`.
    Call(u8, u8),
    /// Direct call by function index. Args in `R(base)..R(base+arity)`.
    /// Result written to `R(base)`.
    CallDirect(u8, u16, u8),
    /// Call host-registered native function. Reserved for future use.
    CallNative(u8, u32, u8),
    /// Method call. `R(base)` is receiver, args in `R(base+1)..`.
    /// Result in `R(base)`.
    CallMethod(u8, u32, u8),

    // ── Null handling ───────────────────────────────────────────────
    /// `R(dst) = R(a) ?? R(b)`
    NullCoalesce(u8, u8, u8),

    // ── String concatenation ────────────────────────────────────────
    /// `R(dst) = str(R(a)) ++ str(R(b))`
    Concat(u8, u8, u8),

    // ── Collections ─────────────────────────────────────────────────
    /// `R(dst) = Array(R(start)..R(start+count))`
    MakeArray(u8, u8, u16),
    /// `R(dst) = Dict` from `R(start)..R(start+2*count)` (key/value pairs)
    MakeDict(u8, u8, u16),
    /// `R(dst) = R(obj)[R(idx)]`
    GetIndex(u8, u8, u8),
    /// `R(obj)[R(idx)] = R(val)`
    SetIndex(u8, u8, u8),
    /// Spread `R(src)` into enclosing collection literal.
    Spread(u8),

    // ── Fields ──────────────────────────────────────────────────────
    /// `R(dst) = R(obj).field[hash]`
    GetField(u8, u8, u32),
    /// `R(obj).field[hash] = R(val)`. Pushes modified struct back if value type.
    SetField(u8, u32, u8),

    // ── Structs & Classes ───────────────────────────────────────────
    /// `R(dst) = Struct(name_hash, R(start)..R(start+count))`
    MakeStruct(u8, u32, u8, u16),
    /// `R(dst) = Class(name_hash, R(start)..R(start+count))`
    MakeClass(u8, u32, u8, u16),

    // ── Closures & Upvalues ─────────────────────────────────────────
    /// `R(dst) = upvalue_cell[idx].borrow().clone()`
    LoadUpvalue(u8, u8),
    /// `*upvalue_cell[idx].borrow_mut() = R(src).cheap_clone()`
    StoreUpvalue(u8, u8),
    /// `R(dst) = Closure(func_idx, captured_upvalues)`
    MakeClosure(u8, u16),
    /// Close upvalue at register `reg` (move stack value to heap cell).
    CloseUpvalue(u8),

    // ── Coroutines ──────────────────────────────────────────────────
    /// Start coroutine. `R(base)` is callee, args follow. Result in `R(base)`.
    StartCoroutine(u8, u8),
    /// Bare yield — suspend for one frame.
    Yield,
    /// Yield for N seconds. Pops `R(src)` as Float.
    YieldSeconds(u8),
    /// Yield for N frames. Pops `R(src)` as Int.
    YieldFrames(u8),
    /// Yield until predicate. `R(src)` is a function reference.
    YieldUntil(u8),
    /// Yield until child coroutine completes. `R(dst) = yield R(src)`.
    YieldCoroutine(u8, u8),

    // ── AoSoA (mobile only) ─────────────────────────────────────────
    #[cfg(feature = "mobile-aosoa")]
    ConvertToAoSoA(u8),

    // ── Quickened instructions (runtime-specialized) ────────────────
    // Generic instructions rewrite to these after type observation.
    // Fast path tries typed op; mismatch deopts back to generic.
    QAddInt(u8, u8, u8),
    QAddFloat(u8, u8, u8),
    QSubInt(u8, u8, u8),
    QSubFloat(u8, u8, u8),
    QMulInt(u8, u8, u8),
    QMulFloat(u8, u8, u8),
    QDivInt(u8, u8, u8),
    QDivFloat(u8, u8, u8),
    QLtInt(u8, u8, u8),
    QLtFloat(u8, u8, u8),
    QLeInt(u8, u8, u8),
    QLeFloat(u8, u8, u8),
    QGtInt(u8, u8, u8),
    QGtFloat(u8, u8, u8),
    QGeInt(u8, u8, u8),
    QGeFloat(u8, u8, u8),
    QEqInt(u8, u8, u8),
    QEqFloat(u8, u8, u8),
    QNeInt(u8, u8, u8),
    QNeFloat(u8, u8, u8),
}
