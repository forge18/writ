use super::error::StackTrace;

#[cfg_attr(not(feature = "debug-hooks"), allow(dead_code))]
pub struct BreakpointContext<'a> {
    pub file: &'a str,
    pub line: u32,
    pub function: &'a str,
    pub stack_trace: &'a StackTrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "debug-hooks"), allow(dead_code))]
pub enum BreakpointAction {
    Continue,
    StepOver,
    StepInto,
    Abort,
}

pub(crate) type BreakpointHandler = Box<dyn Fn(&BreakpointContext) -> BreakpointAction>;
pub(crate) type LineHook = Box<dyn Fn(&str, u32)>;
pub(crate) type CallHook = Box<dyn Fn(&str, &str, u32)>;

#[derive(Debug, Clone, Default)]
#[cfg_attr(not(feature = "debug-hooks"), allow(dead_code))]
pub(crate) enum StepState {
    #[default]
    None,
    /// Pause when line changes and frame depth <= target.
    StepOver {
        target_depth: usize,
    },
    StepInto,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct BreakpointKey {
    pub file: String,
    pub line: u32,
}
