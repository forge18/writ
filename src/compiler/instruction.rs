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

    // ── Type coercion ────────────────────────────────────────────────
    /// `R(dst) = R(src) as f64` — widens I32→F64 or I64→F64
    IntToFloat(u8, u8),

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
    /// Tail call by function index. Reuses the current frame.
    /// Args in `R(base)..R(base+arity)`, result forwarded to caller's result register.
    TailCallDirect(u8, u16, u8),

    // ── Null handling ───────────────────────────────────────────────
    /// `R(dst) = R(a) ?? R(b)`
    NullCoalesce(u8, u8, u8),

    // ── String concatenation ────────────────────────────────────────
    /// `R(dst) = str(R(a)) ++ str(R(b))`
    Concat(u8, u8, u8),

    // ── Collections ─────────────────────────────────────────────────
    /// `R(dst) = Array(R(start)..R(start+count))`
    MakeArray(u8, u8, u8),
    /// `R(dst) = Dict` from `R(start)..R(start+2*count)` (key/value pairs)
    MakeDict(u8, u8, u8),
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
    MakeStruct(u8, u32, u8, u8),
    /// `R(dst) = Class(name_hash, R(start)..R(start+count))`
    MakeClass(u8, u32, u8, u8),

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

impl Instruction {
    /// Encode this instruction into one or two u32 words.
    /// Returns `(word0, Some(word1))` for 2-word instructions, `(word0, None)` for 1-word.
    pub fn encode(&self) -> (u32, Option<u32>) {
        use super::opcode::{a_w, ab_w, abc, abx, op, z};
        match *self {
            // ── Load/Store ──
            Instruction::LoadInt(d, v) => (a_w(op::LoadInt, d), Some(v as u32)),
            Instruction::LoadConstInt(d, idx) => (abx(op::LoadConstInt, d, idx), None),
            Instruction::LoadFloat(d, v) => (a_w(op::LoadFloat, d), Some(v.to_bits())),
            Instruction::LoadConstFloat(d, idx) => (abx(op::LoadConstFloat, d, idx), None),
            Instruction::LoadBool(d, v) => (abc(op::LoadBool, d, v as u8, 0), None),
            Instruction::LoadStr(d, idx) => (a_w(op::LoadStr, d), Some(idx)),
            Instruction::LoadNull(d) => (a_w(op::LoadNull, d), None),
            Instruction::Move(d, s) => (abc(op::Move, d, s, 0), None),
            Instruction::LoadGlobal(d, hash) => (a_w(op::LoadGlobal, d), Some(hash)),

            // ── Arithmetic generic ──
            Instruction::Add(d, a, b) => (abc(op::Add, d, a, b), None),
            Instruction::Sub(d, a, b) => (abc(op::Sub, d, a, b), None),
            Instruction::Mul(d, a, b) => (abc(op::Mul, d, a, b), None),
            Instruction::Div(d, a, b) => (abc(op::Div, d, a, b), None),
            Instruction::Mod(d, a, b) => (abc(op::Mod, d, a, b), None),

            // ── Arithmetic typed ──
            Instruction::AddInt(d, a, b) => (abc(op::AddInt, d, a, b), None),
            Instruction::AddFloat(d, a, b) => (abc(op::AddFloat, d, a, b), None),
            Instruction::SubInt(d, a, b) => (abc(op::SubInt, d, a, b), None),
            Instruction::SubFloat(d, a, b) => (abc(op::SubFloat, d, a, b), None),
            Instruction::MulInt(d, a, b) => (abc(op::MulInt, d, a, b), None),
            Instruction::MulFloat(d, a, b) => (abc(op::MulFloat, d, a, b), None),
            Instruction::DivInt(d, a, b) => (abc(op::DivInt, d, a, b), None),
            Instruction::DivFloat(d, a, b) => (abc(op::DivFloat, d, a, b), None),

            // ── Arithmetic immediate ──
            Instruction::AddIntImm(d, s, imm) => (ab_w(op::AddIntImm, d, s), Some(imm as u32)),
            Instruction::SubIntImm(d, s, imm) => (ab_w(op::SubIntImm, d, s), Some(imm as u32)),

            // ── Type coercion ──
            Instruction::IntToFloat(d, s) => (abc(op::IntToFloat, d, s, 0), None),

            // ── Unary ──
            Instruction::Neg(d, s) => (abc(op::Neg, d, s, 0), None),
            Instruction::Not(d, s) => (abc(op::Not, d, s, 0), None),

            // ── Comparison generic ──
            Instruction::Eq(d, a, b) => (abc(op::Eq, d, a, b), None),
            Instruction::Ne(d, a, b) => (abc(op::Ne, d, a, b), None),
            Instruction::Lt(d, a, b) => (abc(op::Lt, d, a, b), None),
            Instruction::Le(d, a, b) => (abc(op::Le, d, a, b), None),
            Instruction::Gt(d, a, b) => (abc(op::Gt, d, a, b), None),
            Instruction::Ge(d, a, b) => (abc(op::Ge, d, a, b), None),

            // ── Comparison typed ──
            Instruction::EqInt(d, a, b) => (abc(op::EqInt, d, a, b), None),
            Instruction::EqFloat(d, a, b) => (abc(op::EqFloat, d, a, b), None),
            Instruction::NeInt(d, a, b) => (abc(op::NeInt, d, a, b), None),
            Instruction::NeFloat(d, a, b) => (abc(op::NeFloat, d, a, b), None),
            Instruction::LtInt(d, a, b) => (abc(op::LtInt, d, a, b), None),
            Instruction::LtFloat(d, a, b) => (abc(op::LtFloat, d, a, b), None),
            Instruction::LeInt(d, a, b) => (abc(op::LeInt, d, a, b), None),
            Instruction::LeFloat(d, a, b) => (abc(op::LeFloat, d, a, b), None),
            Instruction::GtInt(d, a, b) => (abc(op::GtInt, d, a, b), None),
            Instruction::GtFloat(d, a, b) => (abc(op::GtFloat, d, a, b), None),
            Instruction::GeInt(d, a, b) => (abc(op::GeInt, d, a, b), None),
            Instruction::GeFloat(d, a, b) => (abc(op::GeFloat, d, a, b), None),

            // ── Logical ──
            Instruction::And(d, a, b) => (abc(op::And, d, a, b), None),
            Instruction::Or(d, a, b) => (abc(op::Or, d, a, b), None),

            // ── Control flow ──
            Instruction::Jump(off) => (z(op::Jump), Some(off as u32)),
            Instruction::JumpIfFalsy(s, off) => (a_w(op::JumpIfFalsy, s), Some(off as u32)),
            Instruction::JumpIfTruthy(s, off) => (a_w(op::JumpIfTruthy, s), Some(off as u32)),

            // ── Fused test-and-jump int ──
            Instruction::TestLtInt(a, b, off) => (ab_w(op::TestLtInt, a, b), Some(off as u32)),
            Instruction::TestLeInt(a, b, off) => (ab_w(op::TestLeInt, a, b), Some(off as u32)),
            Instruction::TestGtInt(a, b, off) => (ab_w(op::TestGtInt, a, b), Some(off as u32)),
            Instruction::TestGeInt(a, b, off) => (ab_w(op::TestGeInt, a, b), Some(off as u32)),
            Instruction::TestEqInt(a, b, off) => (ab_w(op::TestEqInt, a, b), Some(off as u32)),
            Instruction::TestNeInt(a, b, off) => (ab_w(op::TestNeInt, a, b), Some(off as u32)),

            // ── Fused test-and-jump int immediate (dead) ──
            Instruction::TestLtIntImm(a, _imm, off) => {
                (ab_w(op::TestLtIntImm, a, 0), Some(off as u32))
            }
            Instruction::TestLeIntImm(a, _imm, off) => {
                (ab_w(op::TestLeIntImm, a, 0), Some(off as u32))
            }
            Instruction::TestGtIntImm(a, _imm, off) => {
                (ab_w(op::TestGtIntImm, a, 0), Some(off as u32))
            }
            Instruction::TestGeIntImm(a, _imm, off) => {
                (ab_w(op::TestGeIntImm, a, 0), Some(off as u32))
            }

            // ── Fused test-and-jump float ──
            Instruction::TestLtFloat(a, b, off) => (ab_w(op::TestLtFloat, a, b), Some(off as u32)),
            Instruction::TestLeFloat(a, b, off) => (ab_w(op::TestLeFloat, a, b), Some(off as u32)),
            Instruction::TestGtFloat(a, b, off) => (ab_w(op::TestGtFloat, a, b), Some(off as u32)),
            Instruction::TestGeFloat(a, b, off) => (ab_w(op::TestGeFloat, a, b), Some(off as u32)),

            // ── Return ──
            Instruction::Return(s) => (a_w(op::Return, s), None),
            Instruction::ReturnNull => (z(op::ReturnNull), None),

            // ── Function calls ──
            Instruction::Call(base, arity) => (abc(op::Call, base, arity, 0), None),
            Instruction::CallDirect(base, func_idx, arity) => {
                (ab_w(op::CallDirect, base, arity), Some(func_idx as u32))
            }
            Instruction::CallNative(base, id, arity) => {
                (ab_w(op::CallNative, base, arity), Some(id))
            }
            Instruction::CallMethod(base, hash, arity) => {
                (ab_w(op::CallMethod, base, arity), Some(hash))
            }
            Instruction::TailCallDirect(base, func_idx, arity) => {
                (ab_w(op::TailCallDirect, base, arity), Some(func_idx as u32))
            }

            // ── Null handling ──
            Instruction::NullCoalesce(d, a, b) => (abc(op::NullCoalesce, d, a, b), None),

            // ── String ──
            Instruction::Concat(d, a, b) => (abc(op::Concat, d, a, b), None),

            // ── Collections ──
            Instruction::MakeArray(d, s, count) => (abc(op::MakeArray, d, s, count), None),
            Instruction::MakeDict(d, s, count) => (abc(op::MakeDict, d, s, count), None),
            Instruction::GetIndex(d, a, b) => (abc(op::GetIndex, d, a, b), None),
            Instruction::SetIndex(a, b, c) => (abc(op::SetIndex, a, b, c), None),
            Instruction::Spread(s) => (a_w(op::Spread, s), None),

            // ── Fields ──
            Instruction::GetField(d, obj, hash) => (ab_w(op::GetField, d, obj), Some(hash)),
            Instruction::SetField(obj, hash, val) => (ab_w(op::SetField, obj, val), Some(hash)),

            // ── Structs & Classes ──
            Instruction::MakeStruct(d, name_hash, start, count) => {
                (abc(op::MakeStruct, d, start, count), Some(name_hash))
            }
            Instruction::MakeClass(d, name_hash, start, count) => {
                (abc(op::MakeClass, d, start, count), Some(name_hash))
            }

            // ── Closures & Upvalues ──
            Instruction::LoadUpvalue(d, idx) => (abc(op::LoadUpvalue, d, idx, 0), None),
            Instruction::StoreUpvalue(v, idx) => (abc(op::StoreUpvalue, v, idx, 0), None),
            Instruction::MakeClosure(d, func_idx) => (abx(op::MakeClosure, d, func_idx), None),
            Instruction::CloseUpvalue(r) => (a_w(op::CloseUpvalue, r), None),

            // ── Coroutines ──
            Instruction::StartCoroutine(base, arity) => {
                (abc(op::StartCoroutine, base, arity, 0), None)
            }
            Instruction::Yield => (z(op::Yield), None),
            Instruction::YieldSeconds(s) => (a_w(op::YieldSeconds, s), None),
            Instruction::YieldFrames(s) => (a_w(op::YieldFrames, s), None),
            Instruction::YieldUntil(s) => (a_w(op::YieldUntil, s), None),
            Instruction::YieldCoroutine(d, s) => (abc(op::YieldCoroutine, d, s, 0), None),

            // ── AoSoA ──
            #[cfg(feature = "mobile-aosoa")]
            Instruction::ConvertToAoSoA(s) => (a_w(op::ConvertToAoSoA, s), None),

            // ── Quickened ──
            Instruction::QAddInt(d, a, b) => (abc(op::QAddInt, d, a, b), None),
            Instruction::QAddFloat(d, a, b) => (abc(op::QAddFloat, d, a, b), None),
            Instruction::QSubInt(d, a, b) => (abc(op::QSubInt, d, a, b), None),
            Instruction::QSubFloat(d, a, b) => (abc(op::QSubFloat, d, a, b), None),
            Instruction::QMulInt(d, a, b) => (abc(op::QMulInt, d, a, b), None),
            Instruction::QMulFloat(d, a, b) => (abc(op::QMulFloat, d, a, b), None),
            Instruction::QDivInt(d, a, b) => (abc(op::QDivInt, d, a, b), None),
            Instruction::QDivFloat(d, a, b) => (abc(op::QDivFloat, d, a, b), None),
            Instruction::QLtInt(d, a, b) => (abc(op::QLtInt, d, a, b), None),
            Instruction::QLtFloat(d, a, b) => (abc(op::QLtFloat, d, a, b), None),
            Instruction::QLeInt(d, a, b) => (abc(op::QLeInt, d, a, b), None),
            Instruction::QLeFloat(d, a, b) => (abc(op::QLeFloat, d, a, b), None),
            Instruction::QGtInt(d, a, b) => (abc(op::QGtInt, d, a, b), None),
            Instruction::QGtFloat(d, a, b) => (abc(op::QGtFloat, d, a, b), None),
            Instruction::QGeInt(d, a, b) => (abc(op::QGeInt, d, a, b), None),
            Instruction::QGeFloat(d, a, b) => (abc(op::QGeFloat, d, a, b), None),
            Instruction::QEqInt(d, a, b) => (abc(op::QEqInt, d, a, b), None),
            Instruction::QEqFloat(d, a, b) => (abc(op::QEqFloat, d, a, b), None),
            Instruction::QNeInt(d, a, b) => (abc(op::QNeInt, d, a, b), None),
            Instruction::QNeFloat(d, a, b) => (abc(op::QNeFloat, d, a, b), None),
        }
    }

    /// Decode a u32 word pair back to an Instruction.
    /// `w1` is only used for 2-word instructions; pass 0 for 1-word.
    pub fn decode(w0: u32, w1: u32) -> Self {
        use super::opcode::{decode_a, decode_b, decode_bx, decode_c, decode_op, op};
        let a = decode_a(w0);
        let b = decode_b(w0);
        let c = decode_c(w0);
        let bx = decode_bx(w0);
        match decode_op(w0) {
            // ── Load/Store ──
            op::LoadInt => Instruction::LoadInt(a, w1 as i32),
            op::LoadConstInt => Instruction::LoadConstInt(a, bx),
            op::LoadFloat => Instruction::LoadFloat(a, f32::from_bits(w1)),
            op::LoadConstFloat => Instruction::LoadConstFloat(a, bx),
            op::LoadBool => Instruction::LoadBool(a, b != 0),
            op::LoadStr => Instruction::LoadStr(a, w1),
            op::LoadNull => Instruction::LoadNull(a),
            op::Move => Instruction::Move(a, b),
            op::LoadGlobal => Instruction::LoadGlobal(a, w1),

            // ── Arithmetic ──
            op::Add => Instruction::Add(a, b, c),
            op::Sub => Instruction::Sub(a, b, c),
            op::Mul => Instruction::Mul(a, b, c),
            op::Div => Instruction::Div(a, b, c),
            op::Mod => Instruction::Mod(a, b, c),
            op::AddInt => Instruction::AddInt(a, b, c),
            op::AddFloat => Instruction::AddFloat(a, b, c),
            op::SubInt => Instruction::SubInt(a, b, c),
            op::SubFloat => Instruction::SubFloat(a, b, c),
            op::MulInt => Instruction::MulInt(a, b, c),
            op::MulFloat => Instruction::MulFloat(a, b, c),
            op::DivInt => Instruction::DivInt(a, b, c),
            op::DivFloat => Instruction::DivFloat(a, b, c),
            op::AddIntImm => Instruction::AddIntImm(a, b, w1 as i32),
            op::SubIntImm => Instruction::SubIntImm(a, b, w1 as i32),
            op::IntToFloat => Instruction::IntToFloat(a, b),
            op::Neg => Instruction::Neg(a, b),
            op::Not => Instruction::Not(a, b),

            // ── Comparison ──
            op::Eq => Instruction::Eq(a, b, c),
            op::Ne => Instruction::Ne(a, b, c),
            op::Lt => Instruction::Lt(a, b, c),
            op::Le => Instruction::Le(a, b, c),
            op::Gt => Instruction::Gt(a, b, c),
            op::Ge => Instruction::Ge(a, b, c),
            op::EqInt => Instruction::EqInt(a, b, c),
            op::EqFloat => Instruction::EqFloat(a, b, c),
            op::NeInt => Instruction::NeInt(a, b, c),
            op::NeFloat => Instruction::NeFloat(a, b, c),
            op::LtInt => Instruction::LtInt(a, b, c),
            op::LtFloat => Instruction::LtFloat(a, b, c),
            op::LeInt => Instruction::LeInt(a, b, c),
            op::LeFloat => Instruction::LeFloat(a, b, c),
            op::GtInt => Instruction::GtInt(a, b, c),
            op::GtFloat => Instruction::GtFloat(a, b, c),
            op::GeInt => Instruction::GeInt(a, b, c),
            op::GeFloat => Instruction::GeFloat(a, b, c),

            // ── Logical ──
            op::And => Instruction::And(a, b, c),
            op::Or => Instruction::Or(a, b, c),

            // ── Control flow ──
            op::Jump => Instruction::Jump(w1 as i32),
            op::JumpIfFalsy => Instruction::JumpIfFalsy(a, w1 as i32),
            op::JumpIfTruthy => Instruction::JumpIfTruthy(a, w1 as i32),

            // ── Fused test-and-jump ──
            op::TestLtInt => Instruction::TestLtInt(a, b, w1 as i32),
            op::TestLeInt => Instruction::TestLeInt(a, b, w1 as i32),
            op::TestGtInt => Instruction::TestGtInt(a, b, w1 as i32),
            op::TestGeInt => Instruction::TestGeInt(a, b, w1 as i32),
            op::TestEqInt => Instruction::TestEqInt(a, b, w1 as i32),
            op::TestNeInt => Instruction::TestNeInt(a, b, w1 as i32),
            op::TestLtIntImm => Instruction::TestLtIntImm(a, 0, w1 as i32),
            op::TestLeIntImm => Instruction::TestLeIntImm(a, 0, w1 as i32),
            op::TestGtIntImm => Instruction::TestGtIntImm(a, 0, w1 as i32),
            op::TestGeIntImm => Instruction::TestGeIntImm(a, 0, w1 as i32),
            op::TestLtFloat => Instruction::TestLtFloat(a, b, w1 as i32),
            op::TestLeFloat => Instruction::TestLeFloat(a, b, w1 as i32),
            op::TestGtFloat => Instruction::TestGtFloat(a, b, w1 as i32),
            op::TestGeFloat => Instruction::TestGeFloat(a, b, w1 as i32),

            // ── Return ──
            op::Return => Instruction::Return(a),
            op::ReturnNull => Instruction::ReturnNull,

            // ── Function calls ──
            op::Call => Instruction::Call(a, b),
            op::CallDirect => Instruction::CallDirect(a, w1 as u16, b),
            op::CallNative => Instruction::CallNative(a, w1, b),
            op::CallMethod => Instruction::CallMethod(a, w1, b),
            op::TailCallDirect => Instruction::TailCallDirect(a, w1 as u16, b),

            // ── Null handling ──
            op::NullCoalesce => Instruction::NullCoalesce(a, b, c),

            // ── String ──
            op::Concat => Instruction::Concat(a, b, c),

            // ── Collections ──
            op::MakeArray => Instruction::MakeArray(a, b, c),
            op::MakeDict => Instruction::MakeDict(a, b, c),
            op::GetIndex => Instruction::GetIndex(a, b, c),
            op::SetIndex => Instruction::SetIndex(a, b, c),
            op::Spread => Instruction::Spread(a),

            // ── Fields ──
            op::GetField => Instruction::GetField(a, b, w1),
            op::SetField => Instruction::SetField(a, w1, b),

            // ── Structs & Classes ──
            op::MakeStruct => Instruction::MakeStruct(a, w1, b, c),
            op::MakeClass => Instruction::MakeClass(a, w1, b, c),

            // ── Closures & Upvalues ──
            op::LoadUpvalue => Instruction::LoadUpvalue(a, b),
            op::StoreUpvalue => Instruction::StoreUpvalue(a, b),
            op::MakeClosure => Instruction::MakeClosure(a, bx),
            op::CloseUpvalue => Instruction::CloseUpvalue(a),

            // ── Coroutines ──
            op::StartCoroutine => Instruction::StartCoroutine(a, b),
            op::Yield => Instruction::Yield,
            op::YieldSeconds => Instruction::YieldSeconds(a),
            op::YieldFrames => Instruction::YieldFrames(a),
            op::YieldUntil => Instruction::YieldUntil(a),
            op::YieldCoroutine => Instruction::YieldCoroutine(a, b),

            // ── Quickened ──
            op::QAddInt => Instruction::QAddInt(a, b, c),
            op::QAddFloat => Instruction::QAddFloat(a, b, c),
            op::QSubInt => Instruction::QSubInt(a, b, c),
            op::QSubFloat => Instruction::QSubFloat(a, b, c),
            op::QMulInt => Instruction::QMulInt(a, b, c),
            op::QMulFloat => Instruction::QMulFloat(a, b, c),
            op::QDivInt => Instruction::QDivInt(a, b, c),
            op::QDivFloat => Instruction::QDivFloat(a, b, c),
            op::QLtInt => Instruction::QLtInt(a, b, c),
            op::QLtFloat => Instruction::QLtFloat(a, b, c),
            op::QLeInt => Instruction::QLeInt(a, b, c),
            op::QLeFloat => Instruction::QLeFloat(a, b, c),
            op::QGtInt => Instruction::QGtInt(a, b, c),
            op::QGtFloat => Instruction::QGtFloat(a, b, c),
            op::QGeInt => Instruction::QGeInt(a, b, c),
            op::QGeFloat => Instruction::QGeFloat(a, b, c),
            op::QEqInt => Instruction::QEqInt(a, b, c),
            op::QEqFloat => Instruction::QEqFloat(a, b, c),
            op::QNeInt => Instruction::QNeInt(a, b, c),
            op::QNeFloat => Instruction::QNeFloat(a, b, c),

            _ => panic!("unknown opcode: {}", decode_op(w0)),
        }
    }

    /// Returns the jump offset for jump-bearing instructions, or `None`.
    pub fn jump_offset(&self) -> Option<i32> {
        match self {
            Instruction::Jump(o)
            | Instruction::JumpIfFalsy(_, o)
            | Instruction::JumpIfTruthy(_, o)
            | Instruction::TestLtInt(_, _, o)
            | Instruction::TestLeInt(_, _, o)
            | Instruction::TestGtInt(_, _, o)
            | Instruction::TestGeInt(_, _, o)
            | Instruction::TestEqInt(_, _, o)
            | Instruction::TestNeInt(_, _, o)
            | Instruction::TestLtIntImm(_, _, o)
            | Instruction::TestLeIntImm(_, _, o)
            | Instruction::TestGtIntImm(_, _, o)
            | Instruction::TestGeIntImm(_, _, o)
            | Instruction::TestLtFloat(_, _, o)
            | Instruction::TestLeFloat(_, _, o)
            | Instruction::TestGtFloat(_, _, o)
            | Instruction::TestGeFloat(_, _, o) => Some(*o),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_size() {
        // Largest variant payload is (u8, i32, i32) = 9 bytes,
        // plus 1-byte discriminant + 2-byte alignment padding = 12 bytes total.
        assert_eq!(
            std::mem::size_of::<Instruction>(),
            12,
            "Instruction enum size changed — check for oversized variants"
        );
    }

    #[test]
    fn encode_decode_roundtrip() {
        // Test representative instructions from each format
        let cases: Vec<Instruction> = vec![
            // ABC (1-word)
            Instruction::Add(3, 7, 12),
            Instruction::QMulFloat(0, 1, 2),
            Instruction::GetIndex(5, 10, 15),
            // ABx (1-word)
            Instruction::LoadConstInt(4, 1000),
            Instruction::MakeClosure(0, 65535),
            // AB (1-word)
            Instruction::Move(1, 2),
            Instruction::Neg(3, 4),
            Instruction::Call(0, 3),
            Instruction::LoadBool(0, true),
            Instruction::LoadBool(1, false),
            // A (1-word)
            Instruction::LoadNull(5),
            Instruction::Return(0),
            // Z (1-word)
            Instruction::ReturnNull,
            Instruction::Yield,
            // A+W (2-word)
            Instruction::LoadInt(2, -42),
            Instruction::LoadFloat(1, 3.14),
            Instruction::LoadStr(0, 999),
            Instruction::LoadGlobal(3, 0xDEAD_BEEF),
            Instruction::JumpIfFalsy(1, 100),
            Instruction::JumpIfTruthy(2, -50),
            // W (2-word)
            Instruction::Jump(200),
            Instruction::Jump(-10),
            // AB+W (2-word)
            Instruction::AddIntImm(0, 1, 42),
            Instruction::SubIntImm(3, 4, -1),
            Instruction::TestLtInt(0, 1, 5),
            Instruction::TestGeFloat(2, 3, -20),
            Instruction::CallDirect(0, 500, 3),
            Instruction::TailCallDirect(1, 100, 2),
            Instruction::CallNative(0, 42, 1),
            Instruction::CallMethod(0, 0xABCD, 2),
            Instruction::GetField(1, 2, 0x1234),
            Instruction::SetField(3, 0x5678, 4),
            // ABC+W (2-word, struct/class)
            Instruction::MakeStruct(0, 0xAAAA, 1, 3),
            Instruction::MakeClass(2, 0xBBBB, 0, 5),
            // Collections
            Instruction::MakeArray(0, 1, 10),
            Instruction::MakeDict(0, 1, 5),
            // Upvalues
            Instruction::LoadUpvalue(0, 3),
            Instruction::StoreUpvalue(1, 2),
            Instruction::CloseUpvalue(5),
            // Coroutines
            Instruction::StartCoroutine(0, 2),
            Instruction::YieldSeconds(1),
            Instruction::YieldCoroutine(0, 1),
        ];

        for instr in cases {
            let (w0, w1_opt) = instr.encode();
            let w1 = w1_opt.unwrap_or(0);
            let decoded = Instruction::decode(w0, w1);
            assert_eq!(instr, decoded, "roundtrip failed for {instr:?}");
        }
    }
}
