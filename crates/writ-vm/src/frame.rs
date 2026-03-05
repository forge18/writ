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
/// Kept as small as possible for cache density.
#[derive(Debug, Clone)]
pub(crate) struct CallFrame {
    /// Which chunk this frame is executing.
    pub chunk_id: ChunkId,
    /// Program counter — index of the next instruction to execute.
    pub pc: usize,
    /// Base stack pointer — index into the operand stack where this
    /// frame's local variables begin.
    pub base: usize,
    /// Whether this frame has a callee slot below `base` that must be
    /// cleaned up on return. True for indirect Call, false for
    /// CallDirect and top-level frames.
    pub has_callee_slot: bool,
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

    /// Returns the stack truncation point on return.
    #[inline(always)]
    pub fn truncate_to(&self) -> usize {
        if self.has_callee_slot {
            self.base - 1
        } else {
            self.base
        }
    }
}
