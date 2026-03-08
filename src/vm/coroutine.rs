use super::frame::CallFrame;
use super::value::Value;

pub type CoroutineId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoroutineState {
    Running,
    /// Waiting for a condition before resuming.
    Suspended,
    Complete,
    /// Triggered by owner destruction or parent cancellation.
    Cancelled,
}

#[derive(Debug, Clone)]
pub enum WaitCondition {
    /// Bare `yield` -- one frame.
    OneFrame,
    /// Accumulated via delta time.
    Seconds {
        remaining: f64,
    },
    Frames {
        remaining: u32,
    },
    Until {
        predicate: Value,
    },
    /// `result_reg` is the frame-relative register for the child's return value.
    Coroutine {
        child_id: CoroutineId,
        result_reg: u8,
    },
}

#[derive(Debug)]
pub struct Coroutine {
    pub id: CoroutineId,
    pub state: CoroutineState,
    pub(crate) stack: Vec<Value>,
    pub(crate) frames: Vec<CallFrame>,
    /// Parallel to `frames`.
    pub(crate) frame_upvalues: Vec<Option<Vec<u32>>>,
    /// Only meaningful when `Suspended`.
    pub(crate) wait: Option<WaitCondition>,
    pub(crate) return_value: Option<Value>,
    /// For structured concurrency.
    pub owner_id: Option<u64>,
    pub(crate) open_upvalues: Vec<Option<u32>>,
    pub(crate) upvalue_store: Vec<Value>,
    /// For cancellation propagation.
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
            frame_upvalues: Vec::new(),
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: Vec::new(),
            upvalue_store: Vec::new(),
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
            frame_upvalues: Vec::new(),
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: Vec::new(),
            upvalue_store: Vec::new(),
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
