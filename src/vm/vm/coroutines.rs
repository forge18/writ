use std::collections::HashSet;
use std::rc::Rc;
#[cfg(feature = "mobile-aosoa")]
use std::cell::RefCell;

use super::super::coroutine::{Coroutine, CoroutineId, CoroutineState, WaitCondition};
use super::super::error::RuntimeError;
use super::super::frame::{CallFrame, ChunkId};
use super::super::value::Value;
use super::{RunResult, VM};

impl VM {
    /// Register-based StartCoroutine.
    pub(super) fn exec_start_coroutine_reg(
        &mut self,
        base: usize,
        base_reg: u8,
        arg_count: u8,
    ) -> Result<(), RuntimeError> {
        let callee_abs = base + base_reg as usize;
        let n = arg_count as usize;
        let callee = self.stack[callee_abs].clone();

        let (func_idx, closure_upvalues) = match &callee {
            Value::Str(s) => {
                let name: &str = s;
                let idx = self
                    .function_map
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.make_error(format!("undefined function '{name}'")))?;
                (idx, None)
            }
            Value::Closure(data) => (data.func_idx, Some(data.upvalues.clone())),
            _ => {
                return Err(self.make_error(format!(
                    "start requires a function, got {}",
                    callee.type_name()
                )));
            }
        };

        let expected_arity = self.functions[func_idx].arity;
        if expected_arity != arg_count {
            return Err(self.make_error(format!(
                "function '{}' expects {} arguments, got {}",
                self.functions[func_idx].name, expected_arity, arg_count
            )));
        }

        let func = &self.functions[func_idx];
        let max_regs = func.max_registers as usize;
        let mut coro_stack = vec![Value::Null; max_regs];
        for (i, slot) in coro_stack.iter_mut().enumerate().take(n) {
            *slot = self.stack[callee_abs + 1 + i].clone();
        }

        let id = self.next_coroutine_id;
        self.next_coroutine_id += 1;

        let has_upvalues = closure_upvalues.is_some();
        let frame = CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: func.max_registers,
            has_rc_values: func.has_rc_values || has_upvalues,
        };

        let coro = Coroutine {
            id,
            state: CoroutineState::Running,
            stack: coro_stack,
            frames: vec![frame],
            frame_upvalues: vec![closure_upvalues],
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: Vec::new(),
            upvalue_store: Vec::new(),
            children: Vec::new(),
        };

        self.coroutines.push(coro);

        if let Some(parent_idx) = self.active_coroutine {
            self.coroutines[parent_idx].children.push(id);
        }

        self.stack[callee_abs] = Value::CoroutineHandle(id);
        Ok(())
    }

    /// Register-based ConvertToAoSoA.
    #[cfg(feature = "mobile-aosoa")]
    pub(super) fn exec_convert_to_aosoa_reg(&mut self, base: usize, src: u8) -> Result<(), RuntimeError> {
        use super::super::aosoa::AoSoAContainer;

        let abs = base + src as usize;
        let val = &self.stack[abs];
        match val {
            Value::Array(arr) => {
                let elements = arr.borrow();

                let first_type = match elements.first() {
                    Some(Value::Struct(s)) => Some(s.layout.type_name.clone()),
                    _ => None,
                };

                let type_name = match first_type {
                    Some(name)
                        if elements.iter().all(
                            |v| matches!(v, Value::Struct(s) if s.layout.type_name == name),
                        ) =>
                    {
                        name
                    }
                    _ => {
                        return Ok(());
                    }
                };

                let layout = match self.struct_layouts.get(&type_name) {
                    Some(layout) => Rc::clone(layout),
                    None => {
                        return Ok(());
                    }
                };

                let mut container = AoSoAContainer::new(layout, elements.len());
                for elem in elements.iter() {
                    if let Value::Struct(s) = elem {
                        container.push(s).map_err(|e| self.make_error(e))?;
                    }
                }
                drop(elements);
                self.stack[abs] = Value::AoSoA(Rc::new(RefCell::new(container)));
            }
            _ => {}
        }
        Ok(())
    }

    // ── Coroutine scheduler (public) ─────────────────────────────

    /// Advances the coroutine scheduler by one frame.
    ///
    /// Cancels all coroutines owned by the given object ID.
    ///
    /// This implements structured concurrency: when a host-side object is
    /// destroyed, all coroutines it owns are automatically cancelled,
    /// including their children.
    pub fn cancel_coroutines_for_owner(&mut self, owner_id: u64) {
        let mut to_cancel: Vec<CoroutineId> = Vec::new();

        // Find all coroutines owned by this owner
        for coro in &self.coroutines {
            if coro.owner_id == Some(owner_id) {
                to_cancel.push(coro.id);
            }
        }

        // Cancel them and their children recursively
        while let Some(id) = to_cancel.pop() {
            if let Some(coro) = self.coroutines.iter_mut().find(|c| c.id == id)
                && coro.state != CoroutineState::Cancelled
                && coro.state != CoroutineState::Complete
            {
                coro.state = CoroutineState::Cancelled;
                to_cancel.extend(coro.children.iter().copied());
            }
        }
    }

    /// Checks wait conditions for all suspended coroutines and resumes
    /// those that are ready. Called once per frame by the host game loop.
    pub fn tick(&mut self, delta: f64) -> Result<(), RuntimeError> {
        // Phase 1: Determine which coroutines are ready to resume.
        // We use index-based iteration to avoid borrowing issues.
        let mut ready_ids: Vec<CoroutineId> = Vec::new();
        let count = self.coroutines.len();

        for i in 0..count {
            match self.coroutines[i].state {
                CoroutineState::Cancelled | CoroutineState::Complete => continue,
                CoroutineState::Running => {
                    ready_ids.push(self.coroutines[i].id);
                }
                CoroutineState::Suspended => {
                    let should_resume = match &mut self.coroutines[i].wait {
                        None => true,
                        Some(WaitCondition::OneFrame) => true,
                        Some(WaitCondition::Seconds { remaining }) => {
                            *remaining -= delta;
                            *remaining <= 1e-6
                        }
                        Some(WaitCondition::Frames { remaining }) => {
                            if *remaining > 0 {
                                *remaining -= 1;
                            }
                            *remaining == 0
                        }
                        Some(WaitCondition::Until { .. }) => {
                            // Will be evaluated during resume
                            true
                        }
                        Some(WaitCondition::Coroutine { child_id, .. }) => {
                            let child_id = *child_id;
                            // Inline check: is the child done?
                            self.coroutines
                                .iter()
                                .find(|c| c.id == child_id)
                                .map(|c| {
                                    matches!(
                                        c.state,
                                        CoroutineState::Complete | CoroutineState::Cancelled
                                    )
                                })
                                .unwrap_or(true)
                        }
                    };
                    if should_resume {
                        ready_ids.push(self.coroutines[i].id);
                    }
                }
            }
        }

        // Phase 2: Resume each ready coroutine
        for id in ready_ids {
            self.resume_coroutine_by_id(id)?;
        }

        // Phase 3: Remove completed and cancelled coroutines.
        // Keep completed coroutines if another coroutine is still waiting on them
        // (WaitCondition::Coroutine), so the parent can read the return value.
        let waited_on: HashSet<CoroutineId> = self
            .coroutines
            .iter()
            .filter_map(|c| match &c.wait {
                Some(WaitCondition::Coroutine { child_id, .. }) => Some(*child_id),
                _ => None,
            })
            .collect();
        self.coroutines.retain(|c| {
            if matches!(c.state, CoroutineState::Complete) && waited_on.contains(&c.id) {
                return true; // keep for parent to read return value
            }
            !matches!(
                c.state,
                CoroutineState::Complete | CoroutineState::Cancelled
            )
        });

        Ok(())
    }

    /// Resumes a coroutine by its ID.
    pub(super) fn resume_coroutine_by_id(&mut self, id: CoroutineId) -> Result<(), RuntimeError> {
        let idx = match self.coroutines.iter().position(|c| c.id == id) {
            Some(i) => i,
            None => return Ok(()), // already removed
        };

        if let Some(WaitCondition::Until { .. }) = &self.coroutines[idx].wait {
            let predicate =
                if let Some(WaitCondition::Until { predicate }) = &self.coroutines[idx].wait {
                    predicate.clone()
                } else {
                    unreachable!()
                };
            let result = self.eval_predicate(&predicate)?;
            if !result {
                // Condition not yet met, stay suspended
                return Ok(());
            }
        }

        // Only YieldCoroutine waits on a child and receives its return value;
        // all other yields leave registers unchanged.
        if let Some(WaitCondition::Coroutine {
            child_id,
            result_reg,
        }) = &self.coroutines[idx].wait
        {
            let child_id = *child_id;
            let result_reg = *result_reg;
            let return_value = self
                .coroutines
                .iter()
                .find(|c| c.id == child_id)
                .and_then(|c| c.return_value.clone())
                .unwrap_or(Value::Null);
            let coro = &mut self.coroutines[idx];
            if let Some(frame) = coro.frames.last() {
                let abs = frame.base + result_reg as usize;
                coro.stack[abs] = return_value;
            }
        }

        let coro = &mut self.coroutines[idx];
        coro.state = CoroutineState::Running;
        coro.wait = None;

        std::mem::swap(&mut self.stack, &mut coro.stack);
        std::mem::swap(&mut self.frames, &mut coro.frames);
        std::mem::swap(&mut self.frame_upvalues, &mut coro.frame_upvalues);
        std::mem::swap(&mut self.open_upvalues, &mut coro.open_upvalues);
        std::mem::swap(&mut self.upvalue_store, &mut coro.upvalue_store);
        self.active_coroutine = Some(idx);

        let result = self.run();

        let coro = &mut self.coroutines[idx];
        std::mem::swap(&mut self.stack, &mut coro.stack);
        std::mem::swap(&mut self.frames, &mut coro.frames);
        std::mem::swap(&mut self.frame_upvalues, &mut coro.frame_upvalues);
        std::mem::swap(&mut self.open_upvalues, &mut coro.open_upvalues);
        std::mem::swap(&mut self.upvalue_store, &mut coro.upvalue_store);
        self.active_coroutine = None;

        match result {
            Ok(RunResult::Yield(wait)) => {
                coro.state = CoroutineState::Suspended;
                coro.wait = Some(wait);
            }
            Ok(RunResult::Return(value)) => {
                coro.state = CoroutineState::Complete;
                coro.return_value = Some(value);
            }
            Err(e) => {
                coro.state = CoroutineState::Complete;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Evaluates a predicate value (function name string or lambda reference).
    /// Returns true if the predicate is satisfied.
    pub(super) fn eval_predicate(&mut self, predicate: &Value) -> Result<bool, RuntimeError> {
        let (func_idx, closure_upvalues) = match predicate {
            Value::Str(s) => {
                let name: &str = s;
                // Try native function first
                if let Some(native) = self.native_functions.get(name) {
                    let body = Rc::clone(&native.body);
                    let result = body(&[]).map_err(|e| self.make_error(e))?;
                    return Ok(!result.is_falsy());
                }
                let idx = self.function_map.get(name).copied().ok_or_else(|| {
                    self.make_error(format!("undefined predicate function '{name}'"))
                })?;
                (idx, None)
            }
            Value::Closure(data) => (data.func_idx, Some(data.upvalues.clone())),
            _ => {
                return Err(self.make_error(format!(
                    "waitUntil expects a function reference, got {}",
                    predicate.type_name()
                )));
            }
        };

        let expected_arity = self.functions[func_idx].arity;
        if expected_arity != 0 {
            return Err(self.make_error(format!(
                "waitUntil predicate must take 0 arguments, '{}' takes {}",
                self.functions[func_idx].name, expected_arity
            )));
        }

        let saved_stack = std::mem::take(&mut self.stack);
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_frame_upvalues = std::mem::take(&mut self.frame_upvalues);
        let saved_upvalues = std::mem::take(&mut self.open_upvalues);
        let saved_active = self.active_coroutine;
        self.active_coroutine = None;

        let max_regs = self.functions[func_idx].max_registers;
        let has_upvalues = closure_upvalues.is_some();
        self.stack.resize(max_regs as usize, Value::Null);
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: max_regs,
            has_rc_values: self.functions[func_idx].has_rc_values || has_upvalues,
        });
        self.frame_upvalues.push(closure_upvalues);

        let result = self.run();

        self.stack = saved_stack;
        self.frames = saved_frames;
        self.frame_upvalues = saved_frame_upvalues;
        self.open_upvalues = saved_upvalues;
        self.active_coroutine = saved_active;

        match result {
            Ok(RunResult::Return(value)) => Ok(!value.is_falsy()),
            Ok(RunResult::Yield(_)) => {
                Err(self.make_error("waitUntil predicate must not yield".to_string()))
            }
            Err(e) => Err(e),
        }
    }

    /// Cancels all coroutines owned by the given object ID.
    pub fn cancel_coroutines_for(&mut self, object_id: u64) {
        let ids_to_cancel: Vec<CoroutineId> = self
            .coroutines
            .iter()
            .filter(|c| c.owner_id == Some(object_id))
            .map(|c| c.id)
            .collect();
        for id in ids_to_cancel {
            self.cancel_coroutine(id);
        }
    }

    /// Cancels a single coroutine and propagates to its children.
    pub(super) fn cancel_coroutine(&mut self, id: CoroutineId) {
        let children: Vec<CoroutineId> = self
            .coroutines
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.children.clone())
            .unwrap_or_default();

        if let Some(coro) = self.coroutines.iter_mut().find(|c| c.id == id) {
            coro.state = CoroutineState::Cancelled;
        }

        for child_id in children {
            self.cancel_coroutine(child_id);
        }
    }

    /// Assigns an owner object to a coroutine (for structured concurrency).
    pub fn set_coroutine_owner(&mut self, coroutine_id: CoroutineId, owner_id: u64) {
        if let Some(coro) = self.coroutines.iter_mut().find(|c| c.id == coroutine_id) {
            coro.owner_id = Some(owner_id);
        }
    }

    /// Returns the ID of the most recently created coroutine.
    pub fn last_coroutine_id(&self) -> Option<CoroutineId> {
        self.coroutines.last().map(|c| c.id)
    }

    /// Returns the number of active coroutines (not completed or cancelled).
    pub fn active_coroutine_count(&self) -> usize {
        self.coroutines
            .iter()
            .filter(|c| {
                !matches!(
                    c.state,
                    CoroutineState::Complete | CoroutineState::Cancelled
                )
            })
            .count()
    }
}
