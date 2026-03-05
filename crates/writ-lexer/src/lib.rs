//! Writ lexer — tokenizes `.writ` source files into a token stream.

mod error;
mod lexer;
mod token;

pub use error::LexError;
pub use lexer::Lexer;
pub use token::{SourceLine, Span, Token, TokenKind, format_error_context};
