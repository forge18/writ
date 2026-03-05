/// Identifies which chunk a CallFrame is executing.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ChunkId {
    /// The main/top-level script chunk.
    Main,
    /// A function chunk, by index in the function table.
    Function(usize),
}

use std::cell::RefCell;
use std::rc::Rc;

use crate::value::Value;

/// A single activation record on the call stack.
///
/// In the register-based VM, each frame owns a window of the value stack
/// `[base .. base + max_registers)`. Parameters occupy the first `arity`
/// registers, locals follow, then temporaries.
#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    /// Which chunk this frame is executing.
    pub chunk_id: ChunkId,
    /// Program counter — index of the next instruction to execute.
    pub pc: usize,
    /// Base stack pointer — index into the operand stack where this
    /// frame's register window begins.
    pub base: usize,
    /// Absolute stack position where the return value should be written.
    /// For the top-level frame, this is unused (set to 0).
    pub result_reg: usize,
    /// Maximum number of registers this frame uses.
    pub max_registers: u8,
    /// True if any register may hold an Rc-bearing value. When false, the
    /// return handler can use `set_len` instead of `truncate` (skips drops).
    pub has_rc_values: bool,
    /// Upvalues for the closure being executed. `None` for non-closure frames.
    pub upvalues: Option<Vec<Rc<RefCell<Value>>>>,
}

impl CallFrame {
    /// Returns the function index if this frame executes a function chunk.
    #[inline(always)]
    pub fn func_index(&self) -> Option<usize> {
        match self.chunk_id {
            ChunkId::Main => None,
            ChunkId::Function(idx) => Some(idx),
        }
    }
}
