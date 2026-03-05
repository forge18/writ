use crate::instruction::{CmpOp, Instruction};

/// Returns the mutable jump offset reference for any jump-bearing instruction,
/// or `None` if the instruction carries no jump offset.
fn jump_offset_mut(instr: &mut Instruction) -> Option<&mut i32> {
    match instr {
        Instruction::Jump(o)
        | Instruction::JumpIfFalse(o)
        | Instruction::JumpIfTrue(o)
        | Instruction::JumpIfFalsePop(o) => Some(o),
        Instruction::CmpLocalIntJump(_, _, _, o) => Some(o),
        Instruction::CmpLocalsJump(_, _, _, o) => Some(o),
        _ => None,
    }
}

/// After removing `removed_count` instructions starting at `removed_start + 1`
/// (i.e., instructions at indices `removed_start+1..removed_start+1+removed_count`
/// were drained), adjust all jump offsets in the instruction stream that cross
/// over the removed region.
///
/// Jump semantics: a jump at position `j` with offset `off` targets
/// `j + 1 + off` (offset is relative to the next instruction).
fn fixup_jumps(instructions: &mut [Instruction], removed_start: usize, removed_count: i32) {
    for j in 0..instructions.len() {
        if let Some(off) = jump_offset_mut(&mut instructions[j]) {
            // The target of this jump (in the NEW coordinate system, before adjustment)
            // was computed assuming the old layout. We need to figure out if the jump
            // crosses the removed region.
            //
            // In the OLD layout (before drain), the jump was at position `old_j`.
            // After drain, positions >= removed_start+1 shifted left by removed_count.
            // So old_j = j if j <= removed_start, else j + removed_count.
            let old_j = if j <= removed_start {
                j as i32
            } else {
                j as i32 + removed_count
            };

            // Old target = old_j + 1 + off (using old offset, which hasn't been adjusted yet)
            let old_target = old_j + 1 + *off;

            // The removed range in old coordinates is [removed_start+1 .. removed_start+1+removed_count)
            let removed_end = removed_start as i32 + 1 + removed_count; // exclusive

            if *off > 0 {
                // Forward jump: if the jump is before the removed region and
                // target is at or after the removed region end, shrink the offset.
                if old_j < removed_start as i32 + 1 && old_target >= removed_end {
                    *off -= removed_count;
                }
            } else if *off < 0 {
                // Backward jump: if the jump is after the removed region and
                // target is before the removed region start, shrink the offset
                // (make it less negative).
                if old_j >= removed_end && old_target <= removed_start as i32 {
                    *off += removed_count;
                }
            }
        }
    }
}

/// Fuse instructions at position `i`, replacing with `fused`, draining
/// `drain_count` instructions after it, and fixing up all jump offsets.
fn fuse(
    instructions: &mut Vec<Instruction>,
    lines: &mut Vec<u32>,
    i: usize,
    fused: Instruction,
    drain_count: usize,
) {
    instructions[i] = fused;
    instructions.drain(i + 1..i + 1 + drain_count);
    lines.drain(i + 1..i + 1 + drain_count);
    fixup_jumps(instructions, i, drain_count as i32);
}

/// Peephole optimization pass — fuses common instruction sequences.
///
/// Called after all instructions are emitted for a chunk.
/// Operates in-place on the instruction and line arrays.
pub fn optimize(instructions: &mut Vec<Instruction>, lines: &mut Vec<u32>) {
    let mut i = 0;
    while i + 4 <= instructions.len() {
        // Pattern: LoadLocal(s) + LoadInt(k) + AddInt + StoreLocal(s) → IncrLocalInt(s, k)
        if let (
            Instruction::LoadLocal(slot1),
            Instruction::LoadInt(imm),
            Instruction::AddInt,
            Instruction::StoreLocal(slot2),
        ) = (
            instructions[i],
            instructions[i + 1],
            instructions[i + 2],
            instructions[i + 3],
        ) && slot1 == slot2
        {
            fuse(
                instructions,
                lines,
                i,
                Instruction::IncrLocalInt(slot1, imm),
                3,
            );
            continue;
        }

        // Pattern: LoadLocal(s) + LoadInt(k) + SubInt + StoreLocal(s) → IncrLocalInt(s, -k)
        if let (
            Instruction::LoadLocal(slot1),
            Instruction::LoadInt(imm),
            Instruction::SubInt,
            Instruction::StoreLocal(slot2),
        ) = (
            instructions[i],
            instructions[i + 1],
            instructions[i + 2],
            instructions[i + 3],
        ) && slot1 == slot2
            && let Some(neg_imm) = imm.checked_neg()
        {
            fuse(
                instructions,
                lines,
                i,
                Instruction::IncrLocalInt(slot1, neg_imm),
                3,
            );
            continue;
        }

        i += 1;
    }

    // Second pass: fuse compare-and-jump patterns
    // Pattern: LoadLocal(s) + LoadInt(k) + LtInt/LeInt/GtInt/GeInt + JumpIfFalse(off) + Pop
    i = 0;
    while i + 5 <= instructions.len() {
        if let (
            Instruction::LoadLocal(slot),
            Instruction::LoadInt(imm),
            cmp_instr,
            Instruction::JumpIfFalse(offset),
            Instruction::Pop,
        ) = (
            instructions[i],
            instructions[i + 1],
            instructions[i + 2],
            instructions[i + 3],
            instructions[i + 4],
        ) {
            let cmp_op = match cmp_instr {
                Instruction::LtInt => Some(CmpOp::Lt),
                Instruction::LeInt => Some(CmpOp::Le),
                Instruction::GtInt => Some(CmpOp::Gt),
                Instruction::GeInt => Some(CmpOp::Ge),
                _ => None,
            };
            if let Some(op) = cmp_op {
                // Fuse 5 instructions → 1. The JumpIfFalse was at position i+3;
                // the fused instruction is at position i (delta = +3). fixup_jumps
                // will then subtract the drain count (4) for jumps crossing the
                // removed region. Net: offset + 3 - 4 = offset - 1.
                let adjusted_offset = offset + 3;
                fuse(
                    instructions,
                    lines,
                    i,
                    Instruction::CmpLocalIntJump(slot, imm, op as u8, adjusted_offset),
                    4,
                );
                continue;
            }
        }
        i += 1;
    }

    // Third pass: fuse LoadLocal + LoadInt + AddInt/SubInt → LoadLocalAddInt/LoadLocalSubInt
    // (only when NOT followed by StoreLocal to same slot — those are IncrLocalInt from pass 1)
    i = 0;
    while i + 3 <= instructions.len() {
        if let (Instruction::LoadLocal(slot), Instruction::LoadInt(imm), arith) =
            (instructions[i], instructions[i + 1], instructions[i + 2])
        {
            let fused_instr = match arith {
                Instruction::AddInt => Some(Instruction::LoadLocalAddInt(slot, imm)),
                Instruction::SubInt => Some(Instruction::LoadLocalSubInt(slot, imm)),
                _ => None,
            };
            if let Some(f) = fused_instr {
                fuse(instructions, lines, i, f, 2);
                continue;
            }
        }
        i += 1;
    }

    // Fourth pass: fuse LoadLocal + LoadLocal + cmp + JumpIfFalsePop → CmpLocalsJump
    // Pattern: LoadLocal(a) + LoadLocal(b) + LtInt/LeInt/GtInt/GeInt + JumpIfFalsePop(off)
    i = 0;
    while i + 4 <= instructions.len() {
        if let (
            Instruction::LoadLocal(slot_a),
            Instruction::LoadLocal(slot_b),
            cmp_instr,
            Instruction::JumpIfFalsePop(offset),
        ) = (
            instructions[i],
            instructions[i + 1],
            instructions[i + 2],
            instructions[i + 3],
        ) {
            let cmp_op = match cmp_instr {
                Instruction::LtInt => Some(CmpOp::Lt),
                Instruction::LeInt => Some(CmpOp::Le),
                Instruction::GtInt => Some(CmpOp::Gt),
                Instruction::GeInt => Some(CmpOp::Ge),
                _ => None,
            };
            if let Some(op) = cmp_op {
                // Fuse 4 instructions → 1. The JumpIfFalsePop was at position
                // i+3; the fused instruction is at position i (delta = +3).
                // fixup_jumps will then subtract drain count (3) for jumps
                // crossing the removed region. Net: offset + 3 - 3 = offset.
                let adjusted_offset = offset + 3;
                fuse(
                    instructions,
                    lines,
                    i,
                    Instruction::CmpLocalsJump(slot_a, slot_b, op as u8, adjusted_offset),
                    3,
                );
                continue;
            }
        }
        i += 1;
    }

    // Fifth pass: fuse LoadLocal + LoadLocal + AddInt/SubInt → AddLocals/SubLocals
    i = 0;
    while i + 3 <= instructions.len() {
        if let (Instruction::LoadLocal(slot_a), Instruction::LoadLocal(slot_b), arith) = (
            instructions[i],
            instructions[i + 1],
            instructions[i + 2],
        ) {
            let fused_instr = match arith {
                Instruction::AddInt => Some(Instruction::AddLocals(slot_a, slot_b)),
                Instruction::SubInt => Some(Instruction::SubLocals(slot_a, slot_b)),
                _ => None,
            };
            if let Some(f) = fused_instr {
                fuse(instructions, lines, i, f, 2);
                continue;
            }
        }
        i += 1;
    }

    // Sixth pass: fuse LoadLocal + Return → ReturnLocal
    i = 0;
    while i + 2 <= instructions.len() {
        if let (Instruction::LoadLocal(slot), Instruction::Return) =
            (instructions[i], instructions[i + 1])
        {
            fuse(instructions, lines, i, Instruction::ReturnLocal(slot), 1);
            continue;
        }
        i += 1;
    }
}
