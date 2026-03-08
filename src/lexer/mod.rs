//! Writ lexer -- tokenizes `.writ` source files into a token stream.

mod error;
#[allow(clippy::module_inception)]
mod lexer;
mod token;

pub use error::LexError;
pub use lexer::Lexer;
pub use token::{SourceLine, Span, Token, TokenKind, format_error_context};
