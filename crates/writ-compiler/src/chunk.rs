use std::collections::HashMap;
use std::rc::Rc;

use crate::instruction::Instruction;

/// A chunk of compiled bytecode with its associated constant data.
///
/// Each chunk represents the bytecode for a single compilation unit
/// (a top-level script or, in later phases, a function body).
#[derive(Debug, Clone)]
pub struct Chunk {
    instructions: Vec<Instruction>,
    strings: Vec<Rc<String>>,
    /// O(1) deduplication index: string content → pool index.
    string_dedup: HashMap<String, u32>,
    /// 64-bit integer constant pool (for LoadConstInt).
    int64_constants: Vec<i64>,
    /// 64-bit float constant pool (for LoadConstFloat).
    float64_constants: Vec<f64>,
    lines: Vec<u32>,
    file: Option<String>,
}

impl Chunk {
    /// Creates a new empty chunk.
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            strings: Vec::new(),
            string_dedup: HashMap::new(),
            int64_constants: Vec::new(),
            float64_constants: Vec::new(),
            lines: Vec::new(),
            file: None,
        }
    }

    /// Appends an instruction and records its source line.
    pub fn write(&mut self, instruction: Instruction, line: u32) {
        self.instructions.push(instruction);
        self.lines.push(line);
    }

    /// Adds a string to the constant pool, deduplicating via O(1) HashMap lookup.
    /// Returns the index of the string in the pool.
    pub fn add_string(&mut self, s: &str) -> u32 {
        if let Some(&index) = self.string_dedup.get(s) {
            return index;
        }
        let index = self.strings.len() as u32;
        let rc = Rc::new(s.to_string());
        self.strings.push(rc);
        self.string_dedup.insert(s.to_string(), index);
        index
    }

    /// Returns the instruction list.
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }

    /// Runs the peephole optimizer on this chunk's instructions.
    pub fn optimize(&mut self) {
        crate::peephole::optimize(&mut self.instructions, &mut self.lines);
    }

    /// Returns the string constant pool as `Rc<String>` references.
    /// Use this from the VM to avoid cloning strings on LoadStr.
    pub fn rc_strings(&self) -> &[Rc<String>] {
        &self.strings
    }

    /// Returns the string constant pool as string slices (for compiler/test use).
    pub fn strings(&self) -> Vec<&str> {
        self.strings.iter().map(|s| s.as_str()).collect()
    }

    /// Adds a 64-bit integer to the constant pool. Returns the index.
    pub fn add_int64(&mut self, v: i64) -> u16 {
        let index = self.int64_constants.len() as u16;
        self.int64_constants.push(v);
        index
    }

    /// Adds a 64-bit float to the constant pool. Returns the index.
    pub fn add_float64(&mut self, v: f64) -> u16 {
        let index = self.float64_constants.len() as u16;
        self.float64_constants.push(v);
        index
    }

    /// Returns the i64 constant pool.
    pub fn int64_constants(&self) -> &[i64] {
        &self.int64_constants
    }

    /// Returns the f64 constant pool.
    pub fn float64_constants(&self) -> &[f64] {
        &self.float64_constants
    }

    /// Returns the source line for the instruction at the given index.
    pub fn line(&self, index: usize) -> u32 {
        self.lines[index]
    }

    /// Emits a jump instruction with its current offset as a placeholder.
    /// Returns the index of the jump instruction for later patching.
    pub fn emit_jump(&mut self, instruction: Instruction, line: u32) -> usize {
        let index = self.instructions.len();
        self.write(instruction, line);
        index
    }

    /// Patches a previously emitted jump instruction so it targets the
    /// current instruction position.
    ///
    /// The offset is `current_position - (jump_index + 1)`, making the
    /// jump relative to the instruction after the jump itself.
    pub fn patch_jump(&mut self, jump_index: usize) {
        let offset = (self.instructions.len() as i32) - (jump_index as i32) - 1;
        match &mut self.instructions[jump_index] {
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
            | Instruction::TestGeFloat(_, _, o) => *o = offset,
            _ => panic!("patch_jump called on non-jump instruction"),
        }
    }

    /// Returns the current instruction count (useful for loop targets).
    pub fn current_offset(&self) -> usize {
        self.instructions.len()
    }

    /// Returns the number of instructions.
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Returns true if the chunk has no instructions.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    /// Sets the source file path associated with this chunk.
    pub fn set_file(&mut self, file: &str) {
        self.file = Some(file.to_string());
    }

    /// Returns the source file path, if set.
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_deduplication() {
        let mut chunk = Chunk::new();
        let i1 = chunk.add_string("hello");
        let i2 = chunk.add_string("world");
        let i3 = chunk.add_string("hello");
        assert_eq!(i1, 0);
        assert_eq!(i2, 1);
        assert_eq!(i3, 0); // deduplicated
        assert_eq!(chunk.strings(), vec!["hello", "world"]);
    }

    #[test]
    fn test_line_tracking() {
        let mut chunk = Chunk::new();
        chunk.write(Instruction::LoadInt(0, 1), 1);
        chunk.write(Instruction::LoadInt(1, 2), 1);
        chunk.write(Instruction::Add(2, 0, 1), 1);
        chunk.write(Instruction::LoadInt(3, 3), 2);
        assert_eq!(chunk.line(0), 1);
        assert_eq!(chunk.line(1), 1);
        assert_eq!(chunk.line(2), 1);
        assert_eq!(chunk.line(3), 2);
    }

    #[test]
    fn test_empty_chunk() {
        let chunk = Chunk::new();
        assert!(chunk.is_empty());
        assert_eq!(chunk.len(), 0);
        assert_eq!(chunk.instructions(), &[]);
    }
}
