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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Span;

    fn span() -> Span {
        Span {
            file: "test.writ".to_string(),
            line: 3,
            column: 5,
            length: 4,
        }
    }

    #[test]
    fn simple_has_no_suggestions() {
        let e = TypeError::simple("bad type".to_string(), span());
        assert_eq!(e.message, "bad type");
        assert!(e.suggestions.is_empty());
        assert!(e.annotation.is_none());
    }

    #[test]
    fn with_suggestions_stores_suggestions() {
        let suggestion = Suggestion {
            message: "did you mean 'health'?".to_string(),
            replacement: Some("health".to_string()),
            span: span(),
        };
        let e = TypeError::with_suggestions("unknown var".to_string(), span(), vec![suggestion]);
        assert_eq!(e.suggestions.len(), 1);
        assert_eq!(e.suggestions[0].message, "did you mean 'health'?");
    }

    #[test]
    fn display_includes_location_and_message() {
        let e = TypeError::simple("type mismatch".to_string(), span());
        let s = e.to_string();
        assert!(s.contains("test.writ"), "missing file: {s}");
        assert!(s.contains("type mismatch"), "missing message: {s}");
        assert!(s.contains('3'), "missing line: {s}");
    }

    #[test]
    fn display_includes_suggestions() {
        let suggestion = Suggestion {
            message: "did you mean 'x'?".to_string(),
            replacement: None,
            span: span(),
        };
        let e = TypeError::with_suggestions("unknown".to_string(), span(), vec![suggestion]);
        let s = e.to_string();
        assert!(s.contains("did you mean 'x'?"), "missing suggestion: {s}");
    }

    #[test]
    fn format_with_source_contains_message() {
        let source = "let x: int = \"hello\"\n";
        let e = TypeError::simple("type mismatch".to_string(), span());
        let out = e.format_with_source(source);
        assert!(out.contains("type mismatch"), "got: {out}");
    }

    #[test]
    fn format_with_source_includes_suggestions() {
        let source = "let y = helth\n";
        let suggestion = Suggestion {
            message: "did you mean 'health'?".to_string(),
            replacement: Some("health".to_string()),
            span: span(),
        };
        let e = TypeError::with_suggestions("unknown var".to_string(), span(), vec![suggestion]);
        let out = e.format_with_source(source);
        assert!(out.contains("did you mean 'health'?"), "got: {out}");
    }

    #[test]
    fn annotation_used_in_format_when_set() {
        // Span at line 1 so the source context renderer can find it
        let sp = Span {
            file: "test.writ".to_string(),
            line: 1,
            column: 16,
            length: 4,
        };
        let source = "let z: float = true\n";
        let mut e = TypeError::simple("type mismatch".to_string(), sp);
        e.annotation = Some("expected float, found bool".to_string());
        let out = e.format_with_source(source);
        // annotation replaces the default message marker under the caret
        assert!(out.contains("expected float, found bool"), "got: {out}");
    }
}
