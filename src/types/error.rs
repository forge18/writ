use crate::lexer::Span;

use super::suggestions::Suggestion;

/// An error encountered during type checking.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
    pub suggestions: Vec<Suggestion>,
    /// Short annotation shown under the error marker (e.g. "expected float, found string").
    pub annotation: Option<String>,
}

impl TypeError {
    /// Creates a `TypeError` with no suggestions.
    pub fn simple(message: String, span: Span) -> Self {
        Self {
            message,
            span,
            suggestions: vec![],
            annotation: None,
        }
    }

    /// Creates a `TypeError` with suggestions.
    pub fn with_suggestions(message: String, span: Span, suggestions: Vec<Suggestion>) -> Self {
        Self {
            message,
            span,
            suggestions,
            annotation: None,
        }
    }

    /// Renders this error with source context using the rich format.
    pub fn format_with_source(&self, source: &str) -> String {
        let annotation = self.annotation.as_deref().unwrap_or(&self.message);
        let mut out =
            crate::lexer::format_error_context(source, &self.span, &self.message, annotation);
        for suggestion in &self.suggestions {
            out.push_str(&format!("  = {}\n", suggestion.message));
        }
        out
    }
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Type error at {}:{}:{}: {}",
            self.span.file, self.span.line, self.span.column, self.message
        )?;
        for suggestion in &self.suggestions {
            write!(f, "\n  = {}", suggestion.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for TypeError {}
