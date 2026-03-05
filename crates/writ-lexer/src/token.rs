/// Source location information attached to every token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub length: u32,
}

/// A single line of source code for error context display.
#[derive(Debug, Clone)]
pub struct SourceLine {
    pub line_number: u32,
    pub text: String,
}

/// Formats a rich error message with source context, similar to Rust/Elm style.
///
/// ```text
/// Error: Type mismatch
///   --> player.writ:34:12
///    |
/// 33 |     health = "full"
///    |              ^^^^^^ expected float, found string
///    |
/// ```
pub fn format_error_context(source: &str, span: &Span, message: &str, annotation: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("Error: {message}\n"));
    out.push_str(&format!(
        "  --> {}:{}:{}\n",
        span.file, span.line, span.column
    ));

    let lines: Vec<&str> = source.lines().collect();
    let error_line = span.line as usize;
    let gutter_width = format!("{}", error_line).len();

    // Blank gutter line
    out.push_str(&format!("{:>gutter_width$} |\n", ""));

    // Show the error line
    if error_line > 0 && error_line <= lines.len() {
        let line_text = lines[error_line - 1];
        out.push_str(&format!("{:>gutter_width$} | {line_text}\n", error_line));

        // Underline annotation
        let col = (span.column as usize).saturating_sub(1);
        let underline_len = (span.length as usize).max(1);
        out.push_str(&format!(
            "{:>gutter_width$} | {:>col$}{} {annotation}\n",
            "",
            "",
            "^".repeat(underline_len),
        ));
    }

    // Blank gutter line
    out.push_str(&format!("{:>gutter_width$} |\n", ""));

    out
}

/// A single token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

/// All possible token types in the Writ language.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Class,
    Trait,
    Enum,
    Struct,
    Func,
    Let,
    Var,
    Const,
    Public,
    Private,
    Static,
    Extends,
    With,
    Import,
    Export,
    Return,
    If,
    Else,
    When,
    While,
    For,
    In,
    Break,
    Continue,
    Is,
    As,
    SelfKeyword,
    Start,
    Yield,
    True,
    False,
    Null,

    // Literals
    IntLiteral(i64),
    FloatLiteral(f64),

    // String interpolation tokens
    StringStart,
    StringLiteral(String),
    InterpolationStart,
    InterpolationEnd,
    StringEnd,
    MultilineStringStart,
    MultilineStringEnd,

    // Identifiers
    Identifier(String),

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Assign,
    EqualEqual,
    BangEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    AmpAmp,
    PipePipe,
    Bang,
    Question,
    QuestionQuestion,
    QuestionDot,
    DotDot,
    DotDotDot,
    DotDotEqual,
    Arrow,
    FatArrow,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    ColonColon,
    Dot,

    // Delimiters
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    Semicolon,

    // Structural
    Newline,
    Eof,
}
