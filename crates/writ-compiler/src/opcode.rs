//! Compact u32 bytecode encoding for the Writ VM.
//!
//! Each instruction is encoded as one or two u32 words:
//! - 1-word:  `[opcode:8][A:8][B:8][C:8]` (ABC), `[opcode:8][A:8][Bx:16]` (ABx)
//! - 2-word:  first word + `[i32/u32]` extra word for immediates, hashes, jumps
//!
//! Little-endian layout: opcode is always `word & 0xFF`.

/// Opcode constants. Numbering is contiguous for dense jump table generation.
#[allow(non_upper_case_globals)]
pub mod op {
    // ── Load/Store (0-8) ──────────────────────────────────────────────
    pub const LoadInt: u8 = 0; // A+W: dst, i32
    pub const LoadConstInt: u8 = 1; // ABx: dst, u16
    pub const LoadFloat: u8 = 2; // A+W: dst, f32-as-u32
    pub const LoadConstFloat: u8 = 3; // ABx: dst, u16
    pub const LoadBool: u8 = 4; // AB: dst, bool
    pub const LoadStr: u8 = 5; // A+W: dst, u32
    pub const LoadNull: u8 = 6; // A: dst
    pub const Move: u8 = 7; // AB: dst, src
    pub const LoadGlobal: u8 = 8; // A+W: dst, u32

    // ── Arithmetic generic (9-13) ─────────────────────────────────────
    pub const Add: u8 = 9; // ABC
    pub const Sub: u8 = 10;
    pub const Mul: u8 = 11;
    pub const Div: u8 = 12;
    pub const Mod: u8 = 13;

    // ── Arithmetic typed (14-21) ──────────────────────────────────────
    pub const AddInt: u8 = 14;
    pub const AddFloat: u8 = 15;
    pub const SubInt: u8 = 16;
    pub const SubFloat: u8 = 17;
    pub const MulInt: u8 = 18;
    pub const MulFloat: u8 = 19;
    pub const DivInt: u8 = 20;
    pub const DivFloat: u8 = 21;

    // ── Arithmetic immediate (22-23) ──────────────────────────────────
    pub const AddIntImm: u8 = 22; // AB+W: dst, src, i32
    pub const SubIntImm: u8 = 23; // AB+W: dst, src, i32

    // ── Type coercion (24) ────────────────────────────────────────────
    pub const IntToFloat: u8 = 24; // AB: dst, src

    // ── Unary (25-26) ─────────────────────────────────────────────────
    pub const Neg: u8 = 25; // AB
    pub const Not: u8 = 26; // AB

    // ── Comparison generic (27-32) ────────────────────────────────────
    pub const Eq: u8 = 27;
    pub const Ne: u8 = 28;
    pub const Lt: u8 = 29;
    pub const Le: u8 = 30;
    pub const Gt: u8 = 31;
    pub const Ge: u8 = 32;

    // ── Comparison typed (33-44) ──────────────────────────────────────
    pub const EqInt: u8 = 33;
    pub const EqFloat: u8 = 34;
    pub const NeInt: u8 = 35;
    pub const NeFloat: u8 = 36;
    pub const LtInt: u8 = 37;
    pub const LtFloat: u8 = 38;
    pub const LeInt: u8 = 39;
    pub const LeFloat: u8 = 40;
    pub const GtInt: u8 = 41;
    pub const GtFloat: u8 = 42;
    pub const GeInt: u8 = 43;
    pub const GeFloat: u8 = 44;

    // ── Logical (45-46) ───────────────────────────────────────────────
    pub const And: u8 = 45;
    pub const Or: u8 = 46;

    // ── Control flow (47-49) ──────────────────────────────────────────
    pub const Jump: u8 = 47; // W: i32
    pub const JumpIfFalsy: u8 = 48; // A+W: src, i32
    pub const JumpIfTruthy: u8 = 49; // A+W: src, i32

    // ── Fused test-and-jump int (50-55) ───────────────────────────────
    pub const TestLtInt: u8 = 50; // AB+W: a, b, i32
    pub const TestLeInt: u8 = 51;
    pub const TestGtInt: u8 = 52;
    pub const TestGeInt: u8 = 53;
    pub const TestEqInt: u8 = 54;
    pub const TestNeInt: u8 = 55;

    // ── Fused test-and-jump int immediate (56-59, DEAD) ───────────────
    // Never emitted by compiler. Kept to preserve ≥117 match arms for LLVM.
    pub const TestLtIntImm: u8 = 56;
    pub const TestLeIntImm: u8 = 57;
    pub const TestGtIntImm: u8 = 58;
    pub const TestGeIntImm: u8 = 59;

    // ── Fused test-and-jump float (60-63) ─────────────────────────────
    pub const TestLtFloat: u8 = 60;
    pub const TestLeFloat: u8 = 61;
    pub const TestGtFloat: u8 = 62;
    pub const TestGeFloat: u8 = 63;

    // ── Return (64-65) ────────────────────────────────────────────────
    pub const Return: u8 = 64; // A: src
    pub const ReturnNull: u8 = 65; // Z: no operands

    // ── Function calls (66-71) ────────────────────────────────────────
    pub const Call: u8 = 66; // AB: base, arity
    pub const CallDirect: u8 = 67; // AB+W: base, arity, u32(func_idx)
    pub const CallNative: u8 = 68; // AB+W: base, arity, u32(id)
    pub const CallMethod: u8 = 69; // AB+W: base, arity, u32(hash)
    pub const TailCallDirect: u8 = 70; // AB+W: base, arity, u32(func_idx)

    // ── Null handling (71) ────────────────────────────────────────────
    pub const NullCoalesce: u8 = 71; // ABC

    // ── String (72) ───────────────────────────────────────────────────
    pub const Concat: u8 = 72; // ABC

    // ── Collections (73-77) ───────────────────────────────────────────
    pub const MakeArray: u8 = 73; // ABC: dst, start, count(u8)
    pub const MakeDict: u8 = 74; // ABC: dst, start, count(u8)
    pub const GetIndex: u8 = 75; // ABC
    pub const SetIndex: u8 = 76; // ABC
    pub const Spread: u8 = 77; // A: src

    // ── Fields (78-79) ────────────────────────────────────────────────
    pub const GetField: u8 = 78; // AB+W: dst, obj, u32(hash)
    pub const SetField: u8 = 79; // AB+W: obj, val, u32(hash)

    // ── Structs & Classes (80-81) ─────────────────────────────────────
    pub const MakeStruct: u8 = 80; // ABC+W: dst, start, count, u32(name_hash)
    pub const MakeClass: u8 = 81; // ABC+W: dst, start, count, u32(name_hash)

    // ── Closures & Upvalues (82-85) ───────────────────────────────────
    pub const LoadUpvalue: u8 = 82; // AB: dst, idx
    pub const StoreUpvalue: u8 = 83; // AB: val, idx
    pub const MakeClosure: u8 = 84; // ABx: dst, u16(func_idx)
    pub const CloseUpvalue: u8 = 85; // A: reg

    // ── Coroutines (86-91) ────────────────────────────────────────────
    pub const StartCoroutine: u8 = 86; // AB: base, arity
    pub const Yield: u8 = 87; // Z
    pub const YieldSeconds: u8 = 88; // A: src
    pub const YieldFrames: u8 = 89; // A: src
    pub const YieldUntil: u8 = 90; // A: src
    pub const YieldCoroutine: u8 = 91; // AB: dst, src

    // ── Quickened (92-111) ────────────────────────────────────────────
    pub const QAddInt: u8 = 92;
    pub const QAddFloat: u8 = 93;
    pub const QSubInt: u8 = 94;
    pub const QSubFloat: u8 = 95;
    pub const QMulInt: u8 = 96;
    pub const QMulFloat: u8 = 97;
    pub const QDivInt: u8 = 98;
    pub const QDivFloat: u8 = 99;
    pub const QLtInt: u8 = 100;
    pub const QLtFloat: u8 = 101;
    pub const QLeInt: u8 = 102;
    pub const QLeFloat: u8 = 103;
    pub const QGtInt: u8 = 104;
    pub const QGtFloat: u8 = 105;
    pub const QGeInt: u8 = 106;
    pub const QGeFloat: u8 = 107;
    pub const QEqInt: u8 = 108;
    pub const QEqFloat: u8 = 109;
    pub const QNeInt: u8 = 110;
    pub const QNeFloat: u8 = 111;

    // ── AoSoA (mobile only) (112) ─────────────────────────────────────
    #[cfg(feature = "mobile-aosoa")]
    pub const ConvertToAoSoA: u8 = 112;

    /// Total opcode count (excluding feature-gated).
    pub const COUNT: u8 = 112;
}

/// Number of u32 words per instruction, indexed by opcode.
/// 1 = single-word, 2 = two-word (has extra u32 payload).
pub static INSTR_WORDS: [u8; 256] = {
    let mut t = [1u8; 256];
    // 2-word: A+W format
    t[op::LoadInt as usize] = 2;
    t[op::LoadFloat as usize] = 2;
    t[op::LoadStr as usize] = 2;
    t[op::LoadGlobal as usize] = 2;
    t[op::JumpIfFalsy as usize] = 2;
    t[op::JumpIfTruthy as usize] = 2;
    // 2-word: W format
    t[op::Jump as usize] = 2;
    // 2-word: AB+W format
    t[op::AddIntImm as usize] = 2;
    t[op::SubIntImm as usize] = 2;
    t[op::TestLtInt as usize] = 2;
    t[op::TestLeInt as usize] = 2;
    t[op::TestGtInt as usize] = 2;
    t[op::TestGeInt as usize] = 2;
    t[op::TestEqInt as usize] = 2;
    t[op::TestNeInt as usize] = 2;
    t[op::TestLtIntImm as usize] = 2; // dead but needs slot
    t[op::TestLeIntImm as usize] = 2;
    t[op::TestGtIntImm as usize] = 2;
    t[op::TestGeIntImm as usize] = 2;
    t[op::TestLtFloat as usize] = 2;
    t[op::TestLeFloat as usize] = 2;
    t[op::TestGtFloat as usize] = 2;
    t[op::TestGeFloat as usize] = 2;
    t[op::CallDirect as usize] = 2;
    t[op::CallNative as usize] = 2;
    t[op::CallMethod as usize] = 2;
    t[op::TailCallDirect as usize] = 2;
    t[op::GetField as usize] = 2;
    t[op::SetField as usize] = 2;
    t[op::MakeStruct as usize] = 2;
    t[op::MakeClass as usize] = 2;
    t
};

// ── Encoding helpers ──────────────────────────────────────────────────

/// Encode a 1-word ABC instruction: `[op:8][a:8][b:8][c:8]`
#[inline(always)]
pub const fn abc(op: u8, a: u8, b: u8, c: u8) -> u32 {
    (op as u32) | ((a as u32) << 8) | ((b as u32) << 16) | ((c as u32) << 24)
}

/// Encode a 1-word ABx instruction: `[op:8][a:8][bx:16]`
#[inline(always)]
pub const fn abx(op: u8, a: u8, bx: u16) -> u32 {
    (op as u32) | ((a as u32) << 8) | ((bx as u32) << 16)
}

/// Encode the first word of a 2-word AB+W instruction: `[op:8][a:8][b:8][_:8]`
/// The second word is the u32/i32 payload.
#[inline(always)]
pub const fn ab_w(op: u8, a: u8, b: u8) -> u32 {
    (op as u32) | ((a as u32) << 8) | ((b as u32) << 16)
}

/// Encode the first word of a 2-word A+W instruction: `[op:8][a:8][_:16]`
#[inline(always)]
pub const fn a_w(op: u8, a: u8) -> u32 {
    (op as u32) | ((a as u32) << 8)
}

/// Encode a zero-operand instruction.
#[inline(always)]
pub const fn z(op: u8) -> u32 {
    op as u32
}

// ── Decoding helpers ──────────────────────────────────────────────────

#[inline(always)]
pub const fn decode_op(w: u32) -> u8 {
    (w & 0xFF) as u8
}

#[inline(always)]
pub const fn decode_a(w: u32) -> u8 {
    ((w >> 8) & 0xFF) as u8
}

#[inline(always)]
pub const fn decode_b(w: u32) -> u8 {
    ((w >> 16) & 0xFF) as u8
}

#[inline(always)]
pub const fn decode_c(w: u32) -> u8 {
    ((w >> 24) & 0xFF) as u8
}

#[inline(always)]
pub const fn decode_bx(w: u32) -> u16 {
    (w >> 16) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abc_roundtrip() {
        let w = abc(op::AddInt, 3, 7, 12);
        assert_eq!(decode_op(w), op::AddInt);
        assert_eq!(decode_a(w), 3);
        assert_eq!(decode_b(w), 7);
        assert_eq!(decode_c(w), 12);
    }

    #[test]
    fn test_abx_roundtrip() {
        let w = abx(op::LoadConstInt, 5, 1024);
        assert_eq!(decode_op(w), op::LoadConstInt);
        assert_eq!(decode_a(w), 5);
        assert_eq!(decode_bx(w), 1024);
    }

    #[test]
    fn test_ab_w_roundtrip() {
        let w0 = ab_w(op::TestLtInt, 2, 4);
        let w1 = 42u32; // offset
        assert_eq!(decode_op(w0), op::TestLtInt);
        assert_eq!(decode_a(w0), 2);
        assert_eq!(decode_b(w0), 4);
        assert_eq!(w1 as i32, 42);
    }

    #[test]
    fn test_instr_words() {
        assert_eq!(INSTR_WORDS[op::Add as usize], 1);
        assert_eq!(INSTR_WORDS[op::LoadInt as usize], 2);
        assert_eq!(INSTR_WORDS[op::Jump as usize], 2);
        assert_eq!(INSTR_WORDS[op::TestLtInt as usize], 2);
        assert_eq!(INSTR_WORDS[op::CallDirect as usize], 2);
        assert_eq!(INSTR_WORDS[op::MakeStruct as usize], 2);
    }

    #[test]
    fn test_opcode_count() {
        // Verify no opcode collisions: all assigned opcodes should be unique
        let mut seen = [false; 256];
        let opcodes = [
            op::LoadInt,
            op::LoadConstInt,
            op::LoadFloat,
            op::LoadConstFloat,
            op::LoadBool,
            op::LoadStr,
            op::LoadNull,
            op::Move,
            op::LoadGlobal,
            op::Add,
            op::Sub,
            op::Mul,
            op::Div,
            op::Mod,
            op::AddInt,
            op::AddFloat,
            op::SubInt,
            op::SubFloat,
            op::MulInt,
            op::MulFloat,
            op::DivInt,
            op::DivFloat,
            op::AddIntImm,
            op::SubIntImm,
            op::IntToFloat,
            op::Neg,
            op::Not,
            op::Eq,
            op::Ne,
            op::Lt,
            op::Le,
            op::Gt,
            op::Ge,
            op::EqInt,
            op::EqFloat,
            op::NeInt,
            op::NeFloat,
            op::LtInt,
            op::LtFloat,
            op::LeInt,
            op::LeFloat,
            op::GtInt,
            op::GtFloat,
            op::GeInt,
            op::GeFloat,
            op::And,
            op::Or,
            op::Jump,
            op::JumpIfFalsy,
            op::JumpIfTruthy,
            op::TestLtInt,
            op::TestLeInt,
            op::TestGtInt,
            op::TestGeInt,
            op::TestEqInt,
            op::TestNeInt,
            op::TestLtIntImm,
            op::TestLeIntImm,
            op::TestGtIntImm,
            op::TestGeIntImm,
            op::TestLtFloat,
            op::TestLeFloat,
            op::TestGtFloat,
            op::TestGeFloat,
            op::Return,
            op::ReturnNull,
            op::Call,
            op::CallDirect,
            op::CallNative,
            op::CallMethod,
            op::TailCallDirect,
            op::NullCoalesce,
            op::Concat,
            op::MakeArray,
            op::MakeDict,
            op::GetIndex,
            op::SetIndex,
            op::Spread,
            op::GetField,
            op::SetField,
            op::MakeStruct,
            op::MakeClass,
            op::LoadUpvalue,
            op::StoreUpvalue,
            op::MakeClosure,
            op::CloseUpvalue,
            op::StartCoroutine,
            op::Yield,
            op::YieldSeconds,
            op::YieldFrames,
            op::YieldUntil,
            op::YieldCoroutine,
            op::QAddInt,
            op::QAddFloat,
            op::QSubInt,
            op::QSubFloat,
            op::QMulInt,
            op::QMulFloat,
            op::QDivInt,
            op::QDivFloat,
            op::QLtInt,
            op::QLtFloat,
            op::QLeInt,
            op::QLeFloat,
            op::QGtInt,
            op::QGtFloat,
            op::QGeInt,
            op::QGeFloat,
            op::QEqInt,
            op::QEqFloat,
            op::QNeInt,
            op::QNeFloat,
        ];
        for &opc in &opcodes {
            assert!(!seen[opc as usize], "duplicate opcode: {opc}");
            seen[opc as usize] = true;
        }
        assert_eq!(opcodes.len(), op::COUNT as usize);
    }
}
