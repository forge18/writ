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

/// A single bytecode instruction for the Writ VM.
///
/// Instructions operate on a stack-based virtual machine. Operands are
/// pushed onto the operand stack, operations pop their inputs and push
/// their results.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Instruction {
    /// Push a 32-bit integer onto the stack.
    LoadInt(i32),
    /// Push a 64-bit integer from the constant pool onto the stack.
    /// `u16` is the index into the chunk's i64 constant pool.
    LoadConstInt(u16),
    /// Push a 32-bit float onto the stack.
    LoadFloat(f32),
    /// Push a 64-bit float from the constant pool onto the stack.
    /// `u16` is the index into the chunk's f64 constant pool.
    LoadConstFloat(u16),
    /// Push a boolean onto the stack.
    LoadBool(bool),
    /// Push a string from the constant pool onto the stack.
    LoadStr(u32),
    /// Push null onto the stack.
    LoadNull,

    /// Push the value of a local variable onto the stack.
    LoadLocal(u8),
    /// Pop the top of stack and store into a local variable slot.
    StoreLocal(u8),

    // Arithmetic
    /// Pop two values, push their sum.
    Add,
    /// Pop two values, push lhs - rhs.
    Sub,
    /// Pop two values, push their product.
    Mul,
    /// Pop two values, push lhs / rhs.
    Div,
    /// Pop two values, push lhs % rhs.
    Mod,

    // Unary
    /// Pop one value, push its negation.
    Neg,
    /// Pop one boolean, push its logical complement.
    Not,

    // Comparison
    /// Pop two values, push true if equal.
    Eq,
    /// Pop two values, push true if not equal.
    Ne,
    /// Pop two values, push true if lhs < rhs.
    Lt,
    /// Pop two values, push true if lhs <= rhs.
    Le,
    /// Pop two values, push true if lhs > rhs.
    Gt,
    /// Pop two values, push true if lhs >= rhs.
    Ge,

    // Logical (non-short-circuit in Phase 9)
    /// Pop two booleans, push logical AND.
    And,
    /// Pop two booleans, push logical OR.
    Or,

    // Control
    /// Return from the current function.
    Return,
    /// Pop and discard the top of stack.
    Pop,

    // ── Phase 10: Control flow ─────────────────────────────────────
    /// Unconditional relative jump. Offset from the NEXT instruction.
    Jump(i32),
    /// Jump if top-of-stack is falsy. Does NOT pop the value.
    JumpIfFalse(i32),
    /// Jump if top-of-stack is truthy. Does NOT pop the value.
    JumpIfTrue(i32),
    /// Jump if top-of-stack is falsy AND pop the value. Fused JumpIfFalse+Pop.
    JumpIfFalsePop(i32),

    // ── Phase 10: Function calls ───────────────────────────────────
    /// Call function. u8 is the argument count. Callee is below args on stack.
    Call(u8),
    /// Direct function call by index — skips LoadGlobal and string-based lookup.
    /// `u16` is the function index, `u8` is the argument count. No callee
    /// value is placed on the stack.
    CallDirect(u16, u8),
    /// Call host-registered native function by ID.
    CallNative(u32),

    // ── Phase 10: Null handling ────────────────────────────────────
    /// Pop [fallback, value]; push value if non-null, else push fallback.
    NullCoalesce,

    // ── Phase 10: String ───────────────────────────────────────────
    /// Pop two values, push their string concatenation.
    Concat,

    // ── Phase 10: Collections ──────────────────────────────────────
    /// Pop N items from stack, create an Array.
    MakeArray(u16),
    /// Pop N key/value pairs from stack, create a Dictionary.
    MakeDict(u16),

    // ── Phase 10: Field/Index access ───────────────────────────────
    /// Get field by name-hash from the object on top of stack.
    GetField(u32),
    /// Set field by name-hash. Stack: [object, value] → [].
    SetField(u32),
    /// Get element at index. Stack: [collection, index] → [value].
    GetIndex,
    /// Set element at index. Stack: [collection, index, value] → [].
    SetIndex,

    // ── Phase 10: Spread ───────────────────────────────────────────
    /// Spread array/dict into enclosing collection literal.
    Spread,

    // ── Phase 13: Coroutines ────────────────────────────────────────
    /// Start a coroutine from a function call. u8 is the argument count.
    /// Stack: [callee, arg0, ..., argN] → [CoroutineHandle].
    StartCoroutine(u8),
    /// Bare yield — suspend for one frame. Pushes Null when resumed.
    Yield,
    /// Yield for N seconds. Pops a Float (seconds) from the stack.
    YieldSeconds,
    /// Yield for N frames. Pops an Int (frame count) from the stack.
    YieldFrames,
    /// Yield until a predicate returns true. Pops a function reference from the stack.
    YieldUntil,
    /// Yield until a child coroutine completes. Pops a CoroutineHandle from the stack.
    /// When the child finishes, pushes the child's return value onto this coroutine's stack.
    YieldCoroutine,

    // ── Closures ──────────────────────────────────────────────────
    /// Push a captured variable (upvalue) onto the stack.
    /// The `u8` is the index into the current frame's upvalue array.
    LoadUpvalue(u8),
    /// Pop the top of stack and store into a captured variable.
    /// The `u8` is the index into the current frame's upvalue array.
    StoreUpvalue(u8),
    /// Create a closure from a compiled function. The `u16` is the function
    /// index in the function table. The VM reads upvalue descriptors from
    /// the `CompiledFunction` to build the upvalue array at runtime.
    MakeClosure(u16),
    /// Close an upvalue — move the value from the stack slot into its heap
    /// cell, then pop the value from the stack. Emitted at scope exit for
    /// locals that have been captured by an inner function.
    CloseUpvalue(u8),

    // ── Phase 19: Structs ───────────────────────────────────────
    /// Construct a struct instance. `u32` is the type name string index,
    /// `u16` is the field count. Pops N field values from the stack
    /// (in declaration order), constructs the struct, and pushes the result.
    MakeStruct(u32, u16),

    // ── Classes ────────────────────────────────────────────────────
    /// Construct a class instance (reference type). `u32` is the type name
    /// string index, `u16` is the field count. Pops N field values from the
    /// stack, creates a `WritClassInstance` wrapped in `Value::Object`, and
    /// pushes the result.
    MakeClass(u32, u16),

    // ── Phase 15: Method calls ────────────────────────────────────
    /// Call a method on a value. `u32` is the method name hash, `u8` is the
    /// argument count. Stack: [receiver, arg0, ..., argN] → [result].
    CallMethod(u32, u8),

    // ── Phase 15: Global variables ────────────────────────────────
    /// Load a global variable by name hash. If not found, falls back to
    /// pushing the name as a string (for function resolution).
    LoadGlobal(u32),

    // ── Phase 19: AoSoA memory layout (mobile only) ─────────────
    /// Convert the top-of-stack Array (of Structs) into an AoSoA container.
    /// Only meaningful for homogeneous struct arrays. Falls back to regular
    /// Array if elements are not all the same struct type.
    #[cfg(feature = "mobile-aosoa")]
    ConvertToAoSoA,

    // ── Typed arithmetic (compiler-guaranteed types) ────────────
    /// Pop two ints, push their sum. Type guaranteed by compiler.
    AddInt,
    /// Pop two floats, push their sum. Type guaranteed by compiler.
    AddFloat,
    /// Pop two ints, push lhs - rhs. Type guaranteed by compiler.
    SubInt,
    /// Pop two floats, push lhs - rhs. Type guaranteed by compiler.
    SubFloat,
    /// Pop two ints, push their product. Type guaranteed by compiler.
    MulInt,
    /// Pop two floats, push their product. Type guaranteed by compiler.
    MulFloat,
    /// Pop two ints, push lhs / rhs. Type guaranteed by compiler.
    DivInt,
    /// Pop two floats, push lhs / rhs. Type guaranteed by compiler.
    DivFloat,

    // ── Typed comparison (compiler-guaranteed types) ────────────
    /// Pop two ints, push true if lhs < rhs.
    LtInt,
    /// Pop two floats, push true if lhs < rhs.
    LtFloat,
    /// Pop two ints, push true if lhs <= rhs.
    LeInt,
    /// Pop two floats, push true if lhs <= rhs.
    LeFloat,
    /// Pop two ints, push true if lhs > rhs.
    GtInt,
    /// Pop two floats, push true if lhs > rhs.
    GtFloat,
    /// Pop two ints, push true if lhs >= rhs.
    GeInt,
    /// Pop two floats, push true if lhs >= rhs.
    GeFloat,
    /// Pop two ints, push true if equal.
    EqInt,
    /// Pop two floats, push true if equal.
    EqFloat,
    /// Pop two ints, push true if not equal.
    NeInt,
    /// Pop two floats, push true if not equal.
    NeFloat,

    // ── Instruction fusion (peephole-optimized) ─────────────────
    /// Increment local by integer immediate: stack[base+slot] += imm.
    IncrLocalInt(u8, i32),
    /// Compare local int to immediate and jump if false.
    /// Encodes: LoadLocal(slot) + LoadInt(imm) + cmp + JumpIfFalse(offset) + Pop.
    /// `u8` = local slot, `i32` = immediate, `u8` = CmpOp encoding, `i32` = jump offset.
    CmpLocalIntJump(u8, i32, u8, i32),
    /// Push stack[base+slot] + imm as Int. Fuses LoadLocal + LoadInt + AddInt.
    LoadLocalAddInt(u8, i32),
    /// Push stack[base+slot] - imm as Int. Fuses LoadLocal + LoadInt + SubInt.
    LoadLocalSubInt(u8, i32),
    /// Return the value of a local directly. Fuses LoadLocal + Return.
    ReturnLocal(u8),
    /// Push stack[base+a] + stack[base+b] as Int. Fuses LoadLocal + LoadLocal + AddInt.
    AddLocals(u8, u8),
    /// Push stack[base+a] - stack[base+b] as Int. Fuses LoadLocal + LoadLocal + SubInt.
    SubLocals(u8, u8),
    /// Compare two locals and jump if false. Fuses LoadLocal + LoadLocal + cmp + JumpIfFalsePop.
    /// `u8` = slot a, `u8` = slot b, `u8` = CmpOp encoding, `i32` = jump offset.
    CmpLocalsJump(u8, u8, u8, i32),

    // ── Quickened instructions (runtime-specialized) ──────────────
    // These are generic instructions that have been rewritten at runtime
    // after observing operand types. They try a fast typed path first;
    // on type mismatch they deopt back to the generic instruction.
    /// Quickened Add — fast path for Int+Int, deopts to generic Add on mismatch.
    QAddInt,
    /// Quickened Add — fast path for Float+Float, deopts to generic Add on mismatch.
    QAddFloat,
    /// Quickened Sub — fast path for Int-Int, deopts to generic Sub on mismatch.
    QSubInt,
    /// Quickened Sub — fast path for Float-Float, deopts to generic Sub on mismatch.
    QSubFloat,
    /// Quickened Mul — fast path for Int*Int, deopts to generic Mul on mismatch.
    QMulInt,
    /// Quickened Mul — fast path for Float*Float, deopts to generic Mul on mismatch.
    QMulFloat,
    /// Quickened Div — fast path for Int/Int, deopts to generic Div on mismatch.
    QDivInt,
    /// Quickened Div — fast path for Float/Float, deopts to generic Div on mismatch.
    QDivFloat,
    /// Quickened Lt — fast path for Int<Int, deopts to generic Lt on mismatch.
    QLtInt,
    /// Quickened Lt — fast path for Float<Float, deopts to generic Lt on mismatch.
    QLtFloat,
    /// Quickened Le — fast path for Int<=Int, deopts to generic Le on mismatch.
    QLeInt,
    /// Quickened Le — fast path for Float<=Float, deopts to generic Le on mismatch.
    QLeFloat,
    /// Quickened Gt — fast path for Int>Int, deopts to generic Gt on mismatch.
    QGtInt,
    /// Quickened Gt — fast path for Float>Float, deopts to generic Gt on mismatch.
    QGtFloat,
    /// Quickened Ge — fast path for Int>=Int, deopts to generic Ge on mismatch.
    QGeInt,
    /// Quickened Ge — fast path for Float>=Float, deopts to generic Ge on mismatch.
    QGeFloat,
    /// Quickened Eq — fast path for Int==Int, deopts to generic Eq on mismatch.
    QEqInt,
    /// Quickened Eq — fast path for Float==Float, deopts to generic Eq on mismatch.
    QEqFloat,
    /// Quickened Ne — fast path for Int!=Int, deopts to generic Ne on mismatch.
    QNeInt,
    /// Quickened Ne — fast path for Float!=Float, deopts to generic Ne on mismatch.
    QNeFloat,
}
