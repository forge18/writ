use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct StackFrame {
    /// `"<script>"` for top-level code.
    pub function: String,
    pub file: String,
    pub line: u32,
    pub is_native: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackTrace {
    /// Innermost (callee) to outermost (caller).
    pub frames: Vec<StackFrame>,
}

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
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
