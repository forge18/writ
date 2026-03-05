use std::cell::RefCell;
use std::rc::Rc;

use crate::frame::CallFrame;
use crate::value::Value;

/// Unique identifier for a coroutine.
pub type CoroutineId = u64;

/// The lifecycle state of a coroutine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoroutineState {
    /// Ready to run or currently executing.
    Running,
    /// Suspended — waiting for a condition before resuming.
    Suspended,
    /// Finished execution — return value available.
    Complete,
    /// Cancelled by owner destruction or parent cancellation.
    Cancelled,
}

/// The wait condition that a suspended coroutine is blocked on.
#[derive(Debug, Clone)]
pub enum WaitCondition {
    /// Suspend for one frame (bare `yield`).
    OneFrame,
    /// Suspend for a number of seconds (accumulated via delta).
    Seconds { remaining: f64 },
    /// Suspend for a number of frames.
    Frames { remaining: u32 },
    /// Suspend until a predicate function returns true.
    Until { predicate: Value },
    /// Suspend until a child coroutine completes.
    /// `result_reg` is the frame-relative register to write the child's return value into.
    Coroutine {
        child_id: CoroutineId,
        result_reg: u8,
    },
}

/// A coroutine — an independent execution context with its own stack.
#[derive(Debug)]
pub struct Coroutine {
    /// Unique ID.
    pub id: CoroutineId,
    /// Current state.
    pub state: CoroutineState,
    /// The operand stack (owned by this coroutine).
    pub(crate) stack: Vec<Value>,
    /// The call stack (owned by this coroutine).
    pub(crate) frames: Vec<CallFrame>,
    /// What this coroutine is waiting on (only meaningful when Suspended).
    pub(crate) wait: Option<WaitCondition>,
    /// Return value after completion.
    pub(crate) return_value: Option<Value>,
    /// The ID of the owning object (for structured concurrency).
    pub owner_id: Option<u64>,
    /// Open upvalues for this coroutine's execution context.
    pub(crate) open_upvalues: Vec<Option<Rc<RefCell<Value>>>>,
    /// Child coroutine IDs (for cancellation propagation).
    pub(crate) children: Vec<CoroutineId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coroutine_initial_state() {
        let coro = Coroutine {
            id: 1,
            state: CoroutineState::Running,
            stack: Vec::new(),
            frames: Vec::new(),
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: Vec::new(),
            children: Vec::new(),
        };
        assert_eq!(coro.state, CoroutineState::Running);
        assert!(coro.return_value.is_none());
        assert!(coro.children.is_empty());
    }

    #[test]
    fn test_coroutine_state_transitions() {
        let mut coro = Coroutine {
            id: 1,
            state: CoroutineState::Running,
            stack: Vec::new(),
            frames: Vec::new(),
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: Vec::new(),
            children: Vec::new(),
        };
        coro.state = CoroutineState::Suspended;
        assert_eq!(coro.state, CoroutineState::Suspended);

        coro.state = CoroutineState::Complete;
        assert_eq!(coro.state, CoroutineState::Complete);

        coro.state = CoroutineState::Cancelled;
        assert_eq!(coro.state, CoroutineState::Cancelled);
    }

    #[test]
    fn test_wait_condition_seconds() {
        let cond = WaitCondition::Seconds { remaining: 2.5 };
        match cond {
            WaitCondition::Seconds { remaining } => {
                assert!((remaining - 2.5).abs() < f64::EPSILON);
            }
            _ => panic!("expected Seconds"),
        }
    }

    #[test]
    fn test_wait_condition_frames() {
        let cond = WaitCondition::Frames { remaining: 10 };
        match cond {
            WaitCondition::Frames { remaining } => assert_eq!(remaining, 10),
            _ => panic!("expected Frames"),
        }
    }
}
