//! Trampoline-based callback system for native functions.
//!
//! Native functions that need to invoke script callbacks (closures/lambdas)
//! return a [`Sequence`] state machine instead of an immediate value. The VM
//! polls the sequence repeatedly: when it needs a script function called, it
//! yields a [`SequenceAction::Call`] request. The VM performs the call through
//! normal frame dispatch, feeds the result back, and polls again.
//!
//! This avoids Rust stack nesting and preserves the constraint that native
//! calls execute synchronously within the caller's frame.

use std::cell::RefCell;
use std::rc::Rc;

use super::value::Value;

/// What a [`Sequence`] step wants the VM executor to do next.
pub enum SequenceAction {
    /// Call a script function/closure and feed the result back on the next poll.
    Call { callee: Value, args: Vec<Value> },
    /// The sequence is complete; here is the final value.
    Done(Value),
    /// The sequence encountered an error.
    Error(String),
}

/// A polling-based state machine driven by the VM executor.
///
/// Implementations hold mutable state (iteration position, accumulator, etc.)
/// and yield [`SequenceAction::Call`] actions whenever they need a script
/// function invoked. The VM drives the sequence to completion via
/// [`VM::drive_sequence`](super::vm::VM).
pub trait Sequence {
    /// Advance the sequence.
    ///
    /// - First call: `last_result` is `None`.
    /// - Subsequent calls: `last_result` is `Some(value)` containing the
    ///   return value from the previous [`SequenceAction::Call`].
    fn poll(&mut self, last_result: Option<Value>) -> SequenceAction;
}

/// Extended return type for native functions that may produce sequences.
pub enum NativeResult {
    /// Immediate value (fast path, same as current behavior).
    Value(Value),
    /// Deferred computation requiring script callbacks.
    Sequence(Box<dyn Sequence>),
}

/// A handle to a Writ function or closure, usable as a native function parameter.
///
/// `WritFn` does **not** invoke the VM directly. Native code uses it to build
/// [`Sequence`] implementations that yield [`SequenceAction::Call`] requests
/// with the stored callee.
///
/// Implements [`FromValue`](super::binding::FromValue), extracting from
/// `Value::Closure` or `Value::Str` (named function reference).
#[derive(Debug, Clone)]
pub struct WritFn {
    callee: Value,
}

impl WritFn {
    /// Creates a new `WritFn` wrapping the given callable value.
    pub fn new(callee: Value) -> Self {
        Self { callee }
    }

    /// Returns a reference to the underlying callable value.
    pub fn callee(&self) -> &Value {
        &self.callee
    }

    /// Consumes the handle and returns the underlying callable value.
    pub fn into_callee(self) -> Value {
        self.callee
    }

    /// Extracts a `WritFn` from an argument slice position.
    ///
    /// Accepts `Value::Closure` or `Value::Str` (named function reference).
    pub fn from_arg(arg: Option<&Value>, pos: usize) -> Result<Self, String> {
        match arg {
            Some(Value::Closure(_) | Value::Str(_)) => Ok(WritFn::new(arg.unwrap().cheap_clone())),
            Some(other) => Err(format!(
                "arg {pos}: expected function or closure, got {}",
                other.type_name()
            )),
            None => Err(format!("arg {pos}: missing callback argument")),
        }
    }
}

// ---------------------------------------------------------------------------
// Pre-built Sequence implementations
// ---------------------------------------------------------------------------

/// Sequence that applies a callback to each element, collecting results.
pub struct MapSequence {
    callee: Value,
    items: Vec<Value>,
    results: Vec<Value>,
    index: usize,
}

impl MapSequence {
    pub fn new(callee: Value, items: Vec<Value>) -> Self {
        let cap = items.len();
        Self {
            callee,
            items,
            results: Vec::with_capacity(cap),
            index: 0,
        }
    }
}

impl Sequence for MapSequence {
    fn poll(&mut self, last_result: Option<Value>) -> SequenceAction {
        if let Some(result) = last_result {
            self.results.push(result);
            self.index += 1;
        }

        if self.index < self.items.len() {
            SequenceAction::Call {
                callee: self.callee.cheap_clone(),
                args: vec![self.items[self.index].clone()],
            }
        } else {
            SequenceAction::Done(Value::Array(Rc::new(RefCell::new(std::mem::take(
                &mut self.results,
            )))))
        }
    }
}

/// Sequence that filters elements by a predicate callback.
pub struct FilterSequence {
    callee: Value,
    items: Vec<Value>,
    results: Vec<Value>,
    index: usize,
}

impl FilterSequence {
    pub fn new(callee: Value, items: Vec<Value>) -> Self {
        Self {
            callee,
            items,
            results: Vec::new(),
            index: 0,
        }
    }
}

impl Sequence for FilterSequence {
    fn poll(&mut self, last_result: Option<Value>) -> SequenceAction {
        if let Some(keep) = last_result
            && !keep.is_falsy()
        {
            self.results.push(self.items[self.index - 1].clone());
        }

        if self.index < self.items.len() {
            let item = self.items[self.index].clone();
            self.index += 1;
            SequenceAction::Call {
                callee: self.callee.cheap_clone(),
                args: vec![item],
            }
        } else {
            SequenceAction::Done(Value::Array(Rc::new(RefCell::new(std::mem::take(
                &mut self.results,
            )))))
        }
    }
}

/// Sequence that reduces elements with an accumulator callback.
pub struct ReduceSequence {
    callee: Value,
    items: Vec<Value>,
    accumulator: Value,
    index: usize,
}

impl ReduceSequence {
    pub fn new(callee: Value, items: Vec<Value>, initial: Value) -> Self {
        Self {
            callee,
            items,
            accumulator: initial,
            index: 0,
        }
    }
}

impl Sequence for ReduceSequence {
    fn poll(&mut self, last_result: Option<Value>) -> SequenceAction {
        if let Some(result) = last_result {
            self.accumulator = result;
            self.index += 1;
        }

        if self.index < self.items.len() {
            SequenceAction::Call {
                callee: self.callee.cheap_clone(),
                args: vec![
                    self.accumulator.cheap_clone(),
                    self.items[self.index].clone(),
                ],
            }
        } else {
            SequenceAction::Done(std::mem::replace(&mut self.accumulator, Value::Null))
        }
    }
}

/// Sequence that calls a callback on each element, discarding results.
pub struct ForEachSequence {
    callee: Value,
    items: Vec<Value>,
    index: usize,
}

impl ForEachSequence {
    pub fn new(callee: Value, items: Vec<Value>) -> Self {
        Self {
            callee,
            items,
            index: 0,
        }
    }
}

impl Sequence for ForEachSequence {
    fn poll(&mut self, last_result: Option<Value>) -> SequenceAction {
        if last_result.is_some() {
            self.index += 1;
        }

        if self.index < self.items.len() {
            SequenceAction::Call {
                callee: self.callee.cheap_clone(),
                args: vec![self.items[self.index].clone()],
            }
        } else {
            SequenceAction::Done(Value::Null)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writ_fn_from_closure() {
        let closure = Value::Closure(Box::new(super::super::value::ClosureData {
            func_idx: 0,
            upvalues: vec![],
        }));
        let wf = WritFn::new(closure.clone());
        assert!(matches!(wf.callee(), Value::Closure(_)));
    }

    #[test]
    fn writ_fn_from_str() {
        let name = Value::Str(Rc::from("double"));
        let wf = WritFn::new(name);
        assert!(matches!(wf.callee(), Value::Str(_)));
    }

    #[test]
    fn map_sequence_empty() {
        let callee = Value::Str(Rc::from("f"));
        let mut seq = MapSequence::new(callee, vec![]);
        match seq.poll(None) {
            SequenceAction::Done(Value::Array(arr)) => {
                assert!(arr.borrow().is_empty());
            }
            _ => panic!("expected Done with empty array"),
        }
    }

    #[test]
    fn map_sequence_requests_calls() {
        let callee = Value::Str(Rc::from("f"));
        let items = vec![Value::I32(1), Value::I32(2)];
        let mut seq = MapSequence::new(callee, items);

        // First poll: should request call for item 0
        match seq.poll(None) {
            SequenceAction::Call { args, .. } => {
                assert_eq!(args[0], Value::I32(1));
            }
            _ => panic!("expected Call"),
        }

        // Feed result, should request call for item 1
        match seq.poll(Some(Value::I32(10))) {
            SequenceAction::Call { args, .. } => {
                assert_eq!(args[0], Value::I32(2));
            }
            _ => panic!("expected Call"),
        }

        // Feed result, should be done
        match seq.poll(Some(Value::I32(20))) {
            SequenceAction::Done(Value::Array(arr)) => {
                let arr = arr.borrow();
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[0], Value::I32(10));
                assert_eq!(arr[1], Value::I32(20));
            }
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn filter_sequence_filters() {
        let callee = Value::Str(Rc::from("f"));
        let items = vec![Value::I32(1), Value::I32(2), Value::I32(3)];
        let mut seq = FilterSequence::new(callee, items);

        // Item 0
        assert!(matches!(seq.poll(None), SequenceAction::Call { .. }));
        // Keep item 0
        assert!(matches!(
            seq.poll(Some(Value::Bool(true))),
            SequenceAction::Call { .. }
        ));
        // Reject item 1
        assert!(matches!(
            seq.poll(Some(Value::Bool(false))),
            SequenceAction::Call { .. }
        ));
        // Keep item 2
        match seq.poll(Some(Value::Bool(true))) {
            SequenceAction::Done(Value::Array(arr)) => {
                let arr = arr.borrow();
                assert_eq!(arr.len(), 2);
                assert_eq!(arr[0], Value::I32(1));
                assert_eq!(arr[1], Value::I32(3));
            }
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn reduce_sequence_accumulates() {
        let callee = Value::Str(Rc::from("f"));
        let items = vec![Value::I32(1), Value::I32(2), Value::I32(3)];
        let mut seq = ReduceSequence::new(callee, items, Value::I32(0));

        // Item 0: call with (0, 1)
        match seq.poll(None) {
            SequenceAction::Call { args, .. } => {
                assert_eq!(args[0], Value::I32(0));
                assert_eq!(args[1], Value::I32(1));
            }
            _ => panic!("expected Call"),
        }

        // Item 1: call with (result, 2)
        match seq.poll(Some(Value::I32(1))) {
            SequenceAction::Call { args, .. } => {
                assert_eq!(args[0], Value::I32(1));
                assert_eq!(args[1], Value::I32(2));
            }
            _ => panic!("expected Call"),
        }

        // Item 2: call with (result, 3)
        match seq.poll(Some(Value::I32(3))) {
            SequenceAction::Call { args, .. } => {
                assert_eq!(args[0], Value::I32(3));
                assert_eq!(args[1], Value::I32(3));
            }
            _ => panic!("expected Call"),
        }

        // Done
        match seq.poll(Some(Value::I32(6))) {
            SequenceAction::Done(v) => assert_eq!(v, Value::I32(6)),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn for_each_sequence_discards_results() {
        let callee = Value::Str(Rc::from("f"));
        let items = vec![Value::I32(1), Value::I32(2)];
        let mut seq = ForEachSequence::new(callee, items);

        assert!(matches!(seq.poll(None), SequenceAction::Call { .. }));
        assert!(matches!(
            seq.poll(Some(Value::Null)),
            SequenceAction::Call { .. }
        ));
        match seq.poll(Some(Value::Null)) {
            SequenceAction::Done(Value::Null) => {}
            _ => panic!("expected Done(Null)"),
        }
    }
}
