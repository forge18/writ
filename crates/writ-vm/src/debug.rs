use crate::error::StackTrace;

/// Context provided to the breakpoint handler when execution pauses.
pub struct BreakpointContext<'a> {
    /// Source file path where the breakpoint was hit.
    pub file: &'a str,
    /// Line number where the breakpoint was hit.
    pub line: u32,
    /// Name of the function containing the breakpoint.
    pub function: &'a str,
    /// Full stack trace at the breakpoint.
    pub stack_trace: &'a StackTrace,
}

/// Action returned by the breakpoint handler to control execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointAction {
    /// Resume normal execution.
    Continue,
    /// Execute the next line at the same frame depth, then pause again.
    StepOver,
    /// Step into the next function call, then pause.
    StepInto,
    /// Terminate the script with an error.
    Abort,
}

/// Type alias for the breakpoint handler callback.
pub(crate) type BreakpointHandler = Box<dyn Fn(&BreakpointContext) -> BreakpointAction>;

/// Type alias for the line debug hook callback.
pub(crate) type LineHook = Box<dyn Fn(&str, u32)>;

/// Type alias for the call/return debug hook callback.
pub(crate) type CallHook = Box<dyn Fn(&str, &str, u32)>;

/// Internal stepping state for the debugger.
#[derive(Debug, Clone, Default)]
pub(crate) enum StepState {
    /// No stepping active — run normally.
    #[default]
    None,
    /// Pause when the line changes and frame depth <= target.
    StepOver { target_depth: usize },
    /// Pause on the very next line change.
    StepInto,
}

/// Identifies a breakpoint location by file and line.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct BreakpointKey {
    pub file: String,
    pub line: u32,
}
