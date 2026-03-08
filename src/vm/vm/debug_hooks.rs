#[cfg(feature = "debug-hooks")]
use super::super::debug::{BreakpointAction, BreakpointContext, BreakpointKey, StepState};
#[cfg(feature = "debug-hooks")]
use super::super::error::RuntimeError;
#[cfg(feature = "debug-hooks")]
use super::super::frame::ChunkId;
use super::VM;
#[cfg(feature = "debug-hooks")]
use super::display_function_name;

impl VM {
    // ── Debug internals ─────────────────────────────────────────────

    /// Checks for line changes, fires debug hooks, and handles breakpoints.
    /// Called once per instruction when `has_debug_hooks` is true.
    #[cfg(feature = "debug-hooks")]
    #[cold]
    #[inline(never)]
    pub(super) fn debug_probe(&mut self, chunk_id: ChunkId, pc: usize) -> Result<(), RuntimeError> {
        let chunk = self.chunk_for(chunk_id);
        let current_line = chunk.line_for_word_offset(pc);
        let current_file = chunk.file().unwrap_or("").to_string();

        // Only act on line changes
        let line_changed = current_line != self.last_line || current_file != self.last_file;
        if !line_changed {
            return Ok(());
        }

        self.last_line = current_line;
        self.last_file = current_file.clone();

        if let Some(ref hook) = self.on_line_hook {
            hook(&current_file, current_line);
        }

        let should_break = match &self.step_state {
            StepState::None => false,
            StepState::StepInto => true,
            StepState::StepOver { target_depth } => self.frames.len() <= *target_depth,
        };

        let at_breakpoint = !self.breakpoints.is_empty()
            && self.breakpoints.contains(&BreakpointKey {
                file: current_file.clone(),
                line: current_line,
            });

        if (should_break || at_breakpoint) && self.breakpoint_handler.is_some() {
            self.step_state = StepState::None;

            // Collect locals before borrowing the handler
            let trace = self.build_stack_trace();
            let fn_name = display_function_name(self.current_frame().func_index(), &self.functions);

            let ctx = BreakpointContext {
                file: &current_file,
                line: current_line,
                function: &fn_name,
                stack_trace: &trace,
            };

            let action = (self.breakpoint_handler.as_ref().unwrap())(&ctx);

            match action {
                BreakpointAction::Continue => {}
                BreakpointAction::StepOver => {
                    self.step_state = StepState::StepOver {
                        target_depth: self.frames.len(),
                    };
                }
                BreakpointAction::StepInto => {
                    self.step_state = StepState::StepInto;
                }
                BreakpointAction::Abort => {
                    return Err(self.make_error("execution aborted by debugger".to_string()));
                }
            }
        }

        Ok(())
    }

    /// Fires the on_call debug hook for the current (just-pushed) frame.
    #[cfg(feature = "debug-hooks")]
    #[cold]
    #[inline(never)]
    pub(super) fn fire_call_hook(&self) {
        if let Some(ref hook) = self.on_call_hook {
            let frame = self.current_frame();
            let chunk = self.chunk_for(frame.chunk_id);
            let file = chunk.file().unwrap_or("");
            let line = if frame.pc > 0 {
                chunk.line(frame.pc - 1)
            } else {
                chunk.line(0)
            };
            let name = display_function_name(frame.func_index(), &self.functions);
            hook(&name, file, line);
        }
    }

    /// Fires the on_return debug hook for the current (about-to-pop) frame.
    #[cfg(feature = "debug-hooks")]
    #[cold]
    #[inline(never)]
    pub(super) fn fire_return_hook(&self) {
        if let Some(ref hook) = self.on_return_hook {
            let frame = self.current_frame();
            let chunk = self.chunk_for(frame.chunk_id);
            let file = chunk.file().unwrap_or("");
            let line = if frame.pc > 0 && frame.pc - 1 < chunk.len() {
                chunk.line(frame.pc - 1)
            } else {
                0
            };
            let name = display_function_name(frame.func_index(), &self.functions);
            hook(&name, file, line);
        }
    }
}
