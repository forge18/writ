use std::collections::HashMap;
use std::rc::Rc;

use crate::instruction::Instruction;
use crate::opcode::INSTR_WORDS;

/// A chunk of compiled bytecode with its associated constant data.
///
/// Each chunk represents the bytecode for a single compilation unit
/// (a top-level script or, in later phases, a function body).
///
/// # Ownership and Quickening
///
/// The VM mutates `code` in-place during quickening (runtime instruction
/// specialization). Each `VM` instance must own its own deep clone of every
/// `Chunk` it executes — never share a single `Chunk` between multiple VMs.
/// `VM::execute_program` and `VM::load_module` both call `chunk.clone()`
/// before storing in `self.main_chunk`. Do not remove that clone as an
/// "optimization": the raw pointers captured by `rebuild_ip_cache` point
/// into the owned `main_chunk.code` and are only valid while that owned
/// copy lives in the VM.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Compact u32-encoded bytecode.
    code: Vec<u32>,
    /// Maps instruction index → word offset in `code`.
    instruction_offsets: Vec<u32>,
    /// Number of logical instructions.
    instr_count: usize,
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
            code: Vec::new(),
            instruction_offsets: Vec::new(),
            instr_count: 0,
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
        let word_offset = self.code.len() as u32;
        self.instruction_offsets.push(word_offset);
        let (w0, w1) = instruction.encode();
        self.code.push(w0);
        if let Some(w) = w1 {
            self.code.push(w);
        }
        self.instr_count += 1;
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

    /// Returns the opcode byte of the last instruction, or `None` if empty.
    pub fn last_opcode(&self) -> Option<u8> {
        if self.instr_count == 0 {
            return None;
        }
        let word_offset = self.instruction_offsets[self.instr_count - 1] as usize;
        Some((self.code[word_offset] & 0xFF) as u8)
    }

    /// Returns the compact u32-encoded bytecode.
    pub fn code(&self) -> &[u32] {
        &self.code
    }

    /// Returns the number of u32 words in the encoded bytecode.
    pub fn word_len(&self) -> usize {
        self.code.len()
    }

    /// Returns the instruction-index-to-word-offset mapping.
    pub fn instruction_offsets(&self) -> &[u32] {
        &self.instruction_offsets
    }

    /// Returns the source line for a word offset in the code vec.
    /// Uses binary search on instruction_offsets (cold path only).
    pub fn line_for_word_offset(&self, word_offset: usize) -> u32 {
        let idx = self
            .instruction_offsets
            .partition_point(|&off| (off as usize) <= word_offset)
            .saturating_sub(1);
        self.lines[idx]
    }

    /// Runs the peephole optimizer on this chunk's instructions,
    /// then rebuilds the compact code vec.
    /// `self_func_idx` is the function index for self-recursive tail call detection.
    pub fn optimize(&mut self, self_func_idx: Option<u16>) {
        // Decode instruction stream from code vec
        let mut instructions = self.decode_all();
        crate::peephole::optimize(&mut instructions, &mut self.lines, self_func_idx);
        self.rebuild_code(&instructions);
        self.instr_count = instructions.len();
    }

    /// Re-encodes an instruction slice into `code` with word-based jump offsets.
    /// Called after peephole optimization modifies the instruction stream.
    fn rebuild_code(&mut self, instructions: &[Instruction]) {
        // Pass 1: compute instruction_offsets
        self.instruction_offsets.clear();
        let mut word_pos = 0u32;
        for instr in instructions {
            self.instruction_offsets.push(word_pos);
            let (_, w1) = instr.encode();
            word_pos += if w1.is_some() { 2 } else { 1 };
        }

        // Pass 2: encode with converted jump offsets (instruction-count → word-count)
        self.code.clear();
        for (i, instr) in instructions.iter().enumerate() {
            let (w0, w1) = instr.encode();
            self.code.push(w0);
            if let Some(mut extra) = w1 {
                // If this is a jump instruction, convert the offset
                if let Some(instr_offset) = instr.jump_offset() {
                    let instr_width = 2u32; // all jump instructions are 2-word
                    let ip_after = self.instruction_offsets[i] + instr_width;
                    let target_instr_idx = (i as i32 + 1 + instr_offset) as usize;
                    let target_word = if target_instr_idx < self.instruction_offsets.len() {
                        self.instruction_offsets[target_instr_idx]
                    } else {
                        word_pos // jump past end
                    };
                    extra = (target_word as i32 - ip_after as i32) as u32;
                }
                self.code.push(extra);
            }
        }
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

    /// Decodes all instructions from the code vec (for tests/debug).
    pub fn decode_all(&self) -> Vec<Instruction> {
        let mut result = Vec::new();
        let mut pos = 0;
        while pos < self.code.len() {
            let w0 = self.code[pos];
            let op = (w0 & 0xFF) as u8;
            let words = INSTR_WORDS[op as usize] as usize;
            let w1 = if words > 1 { self.code[pos + 1] } else { 0 };
            result.push(Instruction::decode(w0, w1));
            pos += words;
        }
        result
    }

    /// Emits a jump instruction with its current offset as a placeholder.
    /// Returns the index of the jump instruction for later patching.
    pub fn emit_jump(&mut self, instruction: Instruction, line: u32) -> usize {
        let index = self.instr_count;
        self.write(instruction, line);
        index
    }

    /// Patches a previously emitted jump instruction so it targets the
    /// current instruction position.
    ///
    /// Stores an instruction-count offset relative to the next instruction.
    /// `rebuild_code()` later converts these to word-count offsets.
    pub fn patch_jump(&mut self, jump_index: usize) {
        let instr_offset = (self.instr_count as i32) - (jump_index as i32) - 1;
        let jump_word_start = self.instruction_offsets[jump_index] as usize;
        let op = (self.code[jump_word_start] & 0xFF) as u8;
        let jump_width = INSTR_WORDS[op as usize] as usize;
        // The offset word is always the last word of the instruction
        self.code[jump_word_start + jump_width - 1] = instr_offset as u32;
    }

    /// Returns the current instruction count (useful for loop targets).
    pub fn current_offset(&self) -> usize {
        self.instr_count
    }

    /// Returns the number of instructions.
    pub fn len(&self) -> usize {
        self.instr_count
    }

    /// Returns true if the chunk has no instructions.
    pub fn is_empty(&self) -> bool {
        self.instr_count == 0
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
        assert_eq!(chunk.decode_all(), &[]);
    }

    #[test]
    fn test_code_vec_sync() {
        let mut chunk = Chunk::new();
        // 1-word: Add(2, 0, 1) → 1 word
        chunk.write(Instruction::Add(2, 0, 1), 1);
        // 2-word: LoadInt(0, 42) → 2 words
        chunk.write(Instruction::LoadInt(0, 42), 1);
        // 1-word: Return(0) → 1 word
        chunk.write(Instruction::Return(0), 1);

        assert_eq!(chunk.len(), 3);
        assert_eq!(chunk.word_len(), 4); // 1 + 2 + 1
        assert_eq!(chunk.instruction_offsets(), &[0, 1, 3]);
    }

    #[test]
    fn test_line_for_word_offset() {
        let mut chunk = Chunk::new();
        chunk.write(Instruction::LoadInt(0, 1), 10); // words 0-1
        chunk.write(Instruction::Add(2, 0, 1), 20); // word 2
        chunk.write(Instruction::LoadInt(3, 3), 30); // words 3-4

        assert_eq!(chunk.line_for_word_offset(0), 10);
        assert_eq!(chunk.line_for_word_offset(1), 10); // still LoadInt's extra word
        assert_eq!(chunk.line_for_word_offset(2), 20);
        assert_eq!(chunk.line_for_word_offset(3), 30);
    }

    #[test]
    fn test_decode_all_roundtrip() {
        let mut chunk = Chunk::new();
        chunk.write(Instruction::LoadInt(0, 42), 1);
        chunk.write(Instruction::LoadInt(1, -7), 1);
        chunk.write(Instruction::AddInt(2, 0, 1), 1);
        chunk.write(Instruction::Return(2), 1);

        let decoded = chunk.decode_all();
        assert_eq!(decoded.len(), 4);
        assert_eq!(decoded[0], Instruction::LoadInt(0, 42));
        assert_eq!(decoded[1], Instruction::LoadInt(1, -7));
        assert_eq!(decoded[2], Instruction::AddInt(2, 0, 1));
        assert_eq!(decoded[3], Instruction::Return(2));
    }
}
