use crate::instruction::Instruction;

/// Returns the mutable jump offset reference for any jump-bearing instruction,
/// or `None` if the instruction carries no jump offset.
fn jump_offset_mut(instr: &mut Instruction) -> Option<&mut i32> {
    match instr {
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
        | Instruction::TestGeFloat(_, _, o) => Some(o),
        _ => None,
    }
}

/// Peephole optimization pass for register-based instructions.
///
/// In the register-based VM, most fusion opportunities (AddLocals, CmpLocalsJump,
/// ReturnLocal, etc.) are absorbed into the native three-address instruction format.
/// The main remaining optimization is dead move elimination.
pub fn optimize(instructions: &mut Vec<Instruction>, _lines: &mut Vec<u32>) {
    // Dead move elimination: Move(A, A) is a no-op
    let mut i = 0;
    while i < instructions.len() {
        if let Instruction::Move(dst, src) = instructions[i]
            && dst == src
        {
            instructions.remove(i);
            _lines.remove(i);
            fixup_jumps(instructions, i.saturating_sub(1), 1);
            continue;
        }
        i += 1;
    }
}

/// After removing `removed_count` instructions starting at `removed_start + 1`,
/// adjust all jump offsets in the instruction stream that cross over the removed region.
fn fixup_jumps(instructions: &mut [Instruction], removed_start: usize, removed_count: i32) {
    for (j, instr) in instructions.iter_mut().enumerate() {
        if let Some(off) = jump_offset_mut(instr) {
            let old_j = if j <= removed_start {
                j as i32
            } else {
                j as i32 + removed_count
            };

            let old_target = old_j + 1 + *off;
            let removed_end = removed_start as i32 + 1 + removed_count;

            if *off > 0 {
                if old_j < removed_start as i32 + 1 && old_target >= removed_end {
                    *off -= removed_count;
                }
            } else if *off < 0 && old_j >= removed_end && old_target <= removed_start as i32 {
                *off += removed_count;
            }
        }
    }
}
