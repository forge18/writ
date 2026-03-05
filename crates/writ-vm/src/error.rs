use std::fmt;

/// A single frame in a stack trace.
#[derive(Debug, Clone, PartialEq)]
pub struct StackFrame {
    /// Function name (or `"<script>"` for top-level code).
    pub function: String,
    /// Source file path.
    pub file: String,
    /// Source line number.
    pub line: u32,
    /// Whether this frame is a host-registered native function.
    pub is_native: bool,
}

/// A stack trace captured at the point of a runtime error.
#[derive(Debug, Clone, PartialEq)]
pub struct StackTrace {
    /// Frames ordered from innermost (callee) to outermost (caller).
    pub frames: Vec<StackFrame>,
}

/// A runtime error produced during VM execution.
#[derive(Debug, Clone)]
pub struct RuntimeError {
    /// Human-readable error message.
    pub message: String,
    /// Stack trace at the point of error.
    pub trace: StackTrace,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Runtime error: {}", self.message)?;
        for frame in &self.trace.frames {
            if frame.is_native {
                writeln!(f, "  at {} [native]", frame.function)?;
            } else {
                writeln!(f, "  at {} ({}:{})", frame.function, frame.file, frame.line)?;
            }
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}
