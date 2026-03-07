use crate::lexer::Span;

/// An error encountered during bytecode compilation.
#[derive(Debug, Clone, PartialEq)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
    /// Short annotation shown under the error marker.
    pub annotation: Option<String>,
}

impl CompileError {
    /// Renders this error with source context using the rich format.
    pub fn format_with_source(&self, source: &str) -> String {
        let annotation = self.annotation.as_deref().unwrap_or(&self.message);
        crate::lexer::format_error_context(source, &self.span, &self.message, annotation)
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Compile error at {}:{}:{}: {}",
            self.span.file, self.span.line, self.span.column, self.message
        )
    }
}

impl std::error::Error for CompileError {}
