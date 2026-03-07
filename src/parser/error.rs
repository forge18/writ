use crate::lexer::Span;

/// An error encountered during parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Parse error at {}:{}:{}: {}",
            self.span.file, self.span.line, self.span.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}
