use lsp_types::{Diagnostic, DiagnosticSeverity};
use writ_lexer::{LexError, Span};
use writ_parser::ParseError;
use writ_types::TypeError;

use crate::convert::span_to_range;

/// Converts a `LexError` to an LSP `Diagnostic`.
pub fn lex_error_to_diagnostic(err: &LexError) -> Diagnostic {
    let span = Span {
        file: err.file.clone(),
        line: err.line,
        column: err.column,
        length: 1,
    };
    Diagnostic {
        range: span_to_range(&span),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("writ".to_string()),
        message: err.message.clone(),
        ..Default::default()
    }
}

/// Converts a `ParseError` to an LSP `Diagnostic`.
pub fn parse_error_to_diagnostic(err: &ParseError) -> Diagnostic {
    Diagnostic {
        range: span_to_range(&err.span),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("writ".to_string()),
        message: err.message.clone(),
        ..Default::default()
    }
}

/// Converts a `TypeError` to an LSP `Diagnostic`.
pub fn type_error_to_diagnostic(err: &TypeError) -> Diagnostic {
    Diagnostic {
        range: span_to_range(&err.span),
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("writ".to_string()),
        message: err.message.clone(),
        ..Default::default()
    }
}
