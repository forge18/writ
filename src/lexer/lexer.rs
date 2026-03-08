use super::error::LexError;
use super::token::{Span, Token, TokenKind};

/// Internal mode for tracking string interpolation state.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LexMode {
    Normal,
    String,
    MultilineString,
    Interpolation { brace_depth: u32 },
}

/// Tokenizes Writ source code into a stream of tokens.
pub struct Lexer<'src> {
    source: &'src [u8],
    file: String,
    pos: usize,
    line: u32,
    column: u32,
    mode_stack: Vec<LexMode>,
    /// Buffer for tokens that need to be emitted before continuing scanning.
    /// Used by string interpolation to queue multiple tokens from a single scan.
    pending: Vec<Token>,
}

impl<'src> Lexer<'src> {
    /// Creates a new lexer for the given source code.
    pub fn new(source: &'src str) -> Self {
        Self::with_file(source, String::new())
    }

    /// Creates a new lexer with a filename for span tracking.
    pub fn with_file(source: &'src str, file: impl Into<String>) -> Self {
        Self {
            source: source.as_bytes(),
            file: file.into(),
            pos: 0,
            line: 1,
            column: 1,
            mode_stack: vec![LexMode::Normal],
            pending: Vec::new(),
        }
    }

    /// Advances and returns the next token.
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        // Drain any pending tokens first (from string interpolation).
        if let Some(token) = self.pending.pop() {
            return Ok(token);
        }

        let mode = self.current_mode().clone();
        match mode {
            LexMode::Normal => self.scan_normal(),
            LexMode::String => self.scan_string_content(false),
            LexMode::MultilineString => self.scan_string_content(true),
            LexMode::Interpolation { brace_depth } => self.scan_interpolation(brace_depth),
        }
    }

    /// Tokenizes the entire source into a vector of tokens.
    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    // -------------------------------------------------------
    // Mode management
    // -------------------------------------------------------

    fn current_mode(&self) -> &LexMode {
        self.mode_stack.last().unwrap_or(&LexMode::Normal)
    }

    fn push_mode(&mut self, mode: LexMode) {
        self.mode_stack.push(mode);
    }

    fn pop_mode(&mut self) {
        self.mode_stack.pop();
    }

    // -------------------------------------------------------
    // Low-level scanning helpers
    // -------------------------------------------------------

    fn peek(&self) -> Option<u8> {
        self.source.get(self.pos).copied()
    }

    fn peek_ahead(&self, n: usize) -> Option<u8> {
        self.source.get(self.pos + n).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let byte = self.source.get(self.pos).copied()?;
        self.pos += 1;
        if byte == b'\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(byte)
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.source.len()
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn make_span(&self, start_line: u32, start_col: u32, length: u32) -> Span {
        Span {
            file: self.file.clone(),
            line: start_line,
            column: start_col,
            length,
        }
    }

    fn make_token(&self, kind: TokenKind, start_line: u32, start_col: u32, length: u32) -> Token {
        Token {
            kind,
            span: self.make_span(start_line, start_col, length),
        }
    }

    fn error(&self, message: impl Into<String>) -> LexError {
        LexError {
            message: message.into(),
            file: self.file.clone(),
            line: self.line,
            column: self.column,
        }
    }

    // -------------------------------------------------------
    // Normal mode scanning
    // -------------------------------------------------------

    fn scan_normal(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace();

        if self.is_at_end() {
            return Ok(self.make_token(TokenKind::Eof, self.line, self.column, 0));
        }

        let start_line = self.line;
        let start_col = self.column;
        let byte = self.peek().unwrap();

        match byte {
            b'\n' => {
                self.advance();
                // Collapse consecutive newlines.
                while self.peek() == Some(b'\n')
                    || self.peek() == Some(b'\r')
                    || self.peek() == Some(b' ')
                    || self.peek() == Some(b'\t')
                {
                    if self.peek() == Some(b'\n') || self.peek() == Some(b'\r') {
                        self.advance();
                    } else {
                        // Skip whitespace between newlines, but stop if we
                        // hit a non-whitespace, non-newline character.
                        let saved_pos = self.pos;
                        let saved_line = self.line;
                        let saved_col = self.column;
                        self.skip_whitespace();
                        if self.peek() == Some(b'\n') || self.peek() == Some(b'\r') {
                            continue;
                        } else {
                            // Restore -- the whitespace is before real content.
                            self.pos = saved_pos;
                            self.line = saved_line;
                            self.column = saved_col;
                            break;
                        }
                    }
                }
                Ok(self.make_token(TokenKind::Newline, start_line, start_col, 1))
            }

            // Delimiters
            b'(' => {
                self.advance();
                Ok(self.make_token(TokenKind::LeftParen, start_line, start_col, 1))
            }
            b')' => {
                self.advance();
                Ok(self.make_token(TokenKind::RightParen, start_line, start_col, 1))
            }
            b'{' => {
                self.advance();
                Ok(self.make_token(TokenKind::LeftBrace, start_line, start_col, 1))
            }
            b'}' => {
                self.advance();
                Ok(self.make_token(TokenKind::RightBrace, start_line, start_col, 1))
            }
            b'[' => {
                self.advance();
                Ok(self.make_token(TokenKind::LeftBracket, start_line, start_col, 1))
            }
            b']' => {
                self.advance();
                Ok(self.make_token(TokenKind::RightBracket, start_line, start_col, 1))
            }
            b',' => {
                self.advance();
                Ok(self.make_token(TokenKind::Comma, start_line, start_col, 1))
            }
            b';' => {
                self.advance();
                Ok(self.make_token(TokenKind::Semicolon, start_line, start_col, 1))
            }

            // Colon -- `:` or `::`
            b':' => {
                self.advance();
                if self.peek() == Some(b':') {
                    self.advance();
                    Ok(self.make_token(TokenKind::ColonColon, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Colon, start_line, start_col, 1))
                }
            }

            // Dot -- `.`, `..`, `...`, `..=`
            b'.' => {
                self.advance();
                if self.peek() == Some(b'.') {
                    self.advance();
                    if self.peek() == Some(b'.') {
                        self.advance();
                        Ok(self.make_token(TokenKind::DotDotDot, start_line, start_col, 3))
                    } else if self.peek() == Some(b'=') {
                        self.advance();
                        Ok(self.make_token(TokenKind::DotDotEqual, start_line, start_col, 3))
                    } else {
                        Ok(self.make_token(TokenKind::DotDot, start_line, start_col, 2))
                    }
                } else {
                    Ok(self.make_token(TokenKind::Dot, start_line, start_col, 1))
                }
            }

            // Plus -- `+`, `+=`
            b'+' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::PlusAssign, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Plus, start_line, start_col, 1))
                }
            }

            // Minus -- `-`, `-=`, `->`
            b'-' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::MinusAssign, start_line, start_col, 2))
                } else if self.peek() == Some(b'>') {
                    self.advance();
                    Ok(self.make_token(TokenKind::Arrow, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Minus, start_line, start_col, 1))
                }
            }

            // Star -- `*`, `*=`
            b'*' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::StarAssign, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Star, start_line, start_col, 1))
                }
            }

            // Slash -- `/`, `/=`, `//` (line comment), `/* */` (block comment)
            b'/' => {
                self.advance();
                if self.peek() == Some(b'/') {
                    self.skip_line_comment();
                    self.next_token()
                } else if self.peek() == Some(b'*') {
                    self.advance();
                    self.skip_block_comment()?;
                    self.next_token()
                } else if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::SlashAssign, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Slash, start_line, start_col, 1))
                }
            }

            // Percent -- `%`, `%=`
            b'%' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::PercentAssign, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Percent, start_line, start_col, 1))
                }
            }

            // Equals -- `=`, `==`, `=>`
            b'=' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::EqualEqual, start_line, start_col, 2))
                } else if self.peek() == Some(b'>') {
                    self.advance();
                    Ok(self.make_token(TokenKind::FatArrow, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Assign, start_line, start_col, 1))
                }
            }

            // Bang -- `!`, `!=`
            b'!' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::BangEqual, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Bang, start_line, start_col, 1))
                }
            }

            // Less -- `<`, `<=`
            b'<' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::LessEqual, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Less, start_line, start_col, 1))
                }
            }

            // Greater -- `>`, `>=`
            b'>' => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Ok(self.make_token(TokenKind::GreaterEqual, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Greater, start_line, start_col, 1))
                }
            }

            // Ampersand -- `&&`
            b'&' => {
                self.advance();
                if self.peek() == Some(b'&') {
                    self.advance();
                    Ok(self.make_token(TokenKind::AmpAmp, start_line, start_col, 2))
                } else {
                    Err(self.error("unexpected character '&', did you mean '&&'?"))
                }
            }

            // Pipe -- `||`
            b'|' => {
                self.advance();
                if self.peek() == Some(b'|') {
                    self.advance();
                    Ok(self.make_token(TokenKind::PipePipe, start_line, start_col, 2))
                } else {
                    Err(self.error("unexpected character '|', did you mean '||'?"))
                }
            }

            // Question -- `?`, `??`, `?.`
            b'?' => {
                self.advance();
                if self.peek() == Some(b'?') {
                    self.advance();
                    Ok(self.make_token(TokenKind::QuestionQuestion, start_line, start_col, 2))
                } else if self.peek() == Some(b'.') {
                    self.advance();
                    Ok(self.make_token(TokenKind::QuestionDot, start_line, start_col, 2))
                } else {
                    Ok(self.make_token(TokenKind::Question, start_line, start_col, 1))
                }
            }

            // String literals
            b'"' => self.scan_string_start(),

            // Numbers
            b'0'..=b'9' => self.scan_number(start_line, start_col),

            // Identifiers and keywords
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.scan_identifier(start_line, start_col),

            _ => {
                let ch = byte as char;
                self.advance();
                Err(self.error(format!("unexpected character '{ch}'")))
            }
        }
    }

    // -------------------------------------------------------
    // Comments
    // -------------------------------------------------------

    fn skip_line_comment(&mut self) {
        // Already consumed the first `/`, skip the second `/`.
        self.advance();
        while let Some(b) = self.peek() {
            if b == b'\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        // Already consumed `/*`.
        loop {
            match self.peek() {
                None => {
                    return Err(self.error("unterminated block comment"));
                }
                Some(b'*') => {
                    self.advance();
                    if self.peek() == Some(b'/') {
                        self.advance();
                        return Ok(());
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // -------------------------------------------------------
    // Identifiers and keywords
    // -------------------------------------------------------

    fn scan_identifier(&mut self, start_line: u32, start_col: u32) -> Result<Token, LexError> {
        let start_pos = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }
        let word =
            std::str::from_utf8(&self.source[start_pos..self.pos]).expect("identifier is ASCII");
        let length = (self.pos - start_pos) as u32;
        let kind = keyword_lookup(word).unwrap_or_else(|| TokenKind::Identifier(word.to_owned()));
        Ok(self.make_token(kind, start_line, start_col, length))
    }

    // -------------------------------------------------------
    // Number literals
    // -------------------------------------------------------

    fn scan_number(&mut self, start_line: u32, start_col: u32) -> Result<Token, LexError> {
        let start_pos = self.pos;

        // Consume integer digits.
        while let Some(b'0'..=b'9') = self.peek() {
            self.advance();
        }

        // Check for float: `.` followed by a digit (not `..` range operator).
        let is_float =
            self.peek() == Some(b'.') && self.peek_ahead(1).is_some_and(|b| b.is_ascii_digit());

        if is_float {
            self.advance(); // consume '.'
            while let Some(b'0'..=b'9') = self.peek() {
                self.advance();
            }
            let text =
                std::str::from_utf8(&self.source[start_pos..self.pos]).expect("number is ASCII");
            let length = (self.pos - start_pos) as u32;
            let value: f64 = text
                .parse()
                .map_err(|_| self.error(format!("invalid float literal: {text}")))?;
            Ok(self.make_token(
                TokenKind::FloatLiteral(value),
                start_line,
                start_col,
                length,
            ))
        } else {
            let text =
                std::str::from_utf8(&self.source[start_pos..self.pos]).expect("number is ASCII");
            let length = (self.pos - start_pos) as u32;
            let value: i64 = text
                .parse()
                .map_err(|_| self.error(format!("invalid integer literal: {text}")))?;
            Ok(self.make_token(TokenKind::IntLiteral(value), start_line, start_col, length))
        }
    }

    // -------------------------------------------------------
    // String scanning
    // -------------------------------------------------------

    fn scan_string_start(&mut self) -> Result<Token, LexError> {
        let start_line = self.line;
        let start_col = self.column;

        // Consume the opening `"`.
        self.advance();

        // Check for multiline string `"""`.
        if self.peek() == Some(b'"') && self.peek_ahead(1) == Some(b'"') {
            self.advance(); // second "
            self.advance(); // third "
            self.push_mode(LexMode::MultilineString);
            Ok(self.make_token(TokenKind::MultilineStringStart, start_line, start_col, 3))
        } else {
            self.push_mode(LexMode::String);
            Ok(self.make_token(TokenKind::StringStart, start_line, start_col, 1))
        }
    }

    fn scan_string_content(&mut self, multiline: bool) -> Result<Token, LexError> {
        // Drain pending tokens first.
        if let Some(token) = self.pending.pop() {
            return Ok(token);
        }

        let start_line = self.line;
        let start_col = self.column;
        let mut content = String::new();

        loop {
            match self.peek() {
                None => {
                    return Err(self.error("unterminated string literal"));
                }
                Some(b'"') => {
                    if multiline {
                        // Check for closing `"""`.
                        if self.peek_ahead(1) == Some(b'"') && self.peek_ahead(2) == Some(b'"') {
                            // Emit accumulated content, then closing token.
                            let end_line = self.line;
                            let end_col = self.column;
                            self.advance(); // "
                            self.advance(); // "
                            self.advance(); // "
                            self.pop_mode();

                            let end_token = self.make_token(
                                TokenKind::MultilineStringEnd,
                                end_line,
                                end_col,
                                3,
                            );

                            if content.is_empty() {
                                return Ok(end_token);
                            }
                            let length = content.len() as u32;
                            self.pending.push(end_token);
                            return Ok(self.make_token(
                                TokenKind::StringLiteral(content),
                                start_line,
                                start_col,
                                length,
                            ));
                        }
                        // Single or double `"` inside multiline -- literal content.
                        content.push(self.advance().unwrap() as char);
                    } else {
                        // Closing `"` for single-line string.
                        let end_line = self.line;
                        let end_col = self.column;
                        self.advance();
                        self.pop_mode();

                        let end_token = self.make_token(TokenKind::StringEnd, end_line, end_col, 1);

                        if content.is_empty() {
                            return Ok(end_token);
                        }
                        let length = content.len() as u32;
                        self.pending.push(end_token);
                        return Ok(self.make_token(
                            TokenKind::StringLiteral(content),
                            start_line,
                            start_col,
                            length,
                        ));
                    }
                }
                Some(b'\\') => {
                    self.advance(); // consume '\'
                    let escaped = self.scan_escape()?;
                    content.push(escaped);
                }
                Some(b'$') => {
                    // Check for interpolation.
                    if self
                        .peek_ahead(1)
                        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                    {
                        // $identifier interpolation.
                        return self.scan_simple_interpolation(content, start_line, start_col);
                    } else if self.peek_ahead(1) == Some(b'{') {
                        // ${expr} interpolation.
                        return self.scan_expr_interpolation(content, start_line, start_col);
                    }
                    // Bare `$` not followed by ident or `{` -- literal.
                    content.push(self.advance().unwrap() as char);
                }
                Some(b'\n') if !multiline => {
                    return Err(self.error("unterminated string literal"));
                }
                Some(b) => {
                    self.advance();
                    content.push(b as char);
                }
            }
        }
    }

    fn scan_escape(&mut self) -> Result<char, LexError> {
        match self.peek() {
            Some(b'n') => {
                self.advance();
                Ok('\n')
            }
            Some(b't') => {
                self.advance();
                Ok('\t')
            }
            Some(b'\\') => {
                self.advance();
                Ok('\\')
            }
            Some(b'"') => {
                self.advance();
                Ok('"')
            }
            Some(b'$') => {
                self.advance();
                Ok('$')
            }
            Some(b) => {
                self.advance();
                Err(self.error(format!("unknown escape sequence: \\{}", b as char)))
            }
            None => Err(self.error("unterminated escape sequence")),
        }
    }

    /// Handles `$identifier` inside a string.
    /// Emits: StringLiteral (if content), InterpolationStart, Identifier, InterpolationEnd.
    fn scan_simple_interpolation(
        &mut self,
        content: String,
        start_line: u32,
        start_col: u32,
    ) -> Result<Token, LexError> {
        // Consume the `$`.
        let interp_line = self.line;
        let interp_col = self.column;
        self.advance();

        // Scan the identifier.
        let ident_line = self.line;
        let ident_col = self.column;
        let ident_start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.advance();
            } else {
                break;
            }
        }
        let ident = std::str::from_utf8(&self.source[ident_start..self.pos])
            .expect("identifier is ASCII")
            .to_owned();
        let ident_len = ident.len() as u32;

        // Queue tokens in reverse order (we pop from the back).
        let interp_end = self.make_token(TokenKind::InterpolationEnd, self.line, self.column, 0);
        let ident_token = self.make_token(
            TokenKind::Identifier(ident),
            ident_line,
            ident_col,
            ident_len,
        );
        let interp_start =
            self.make_token(TokenKind::InterpolationStart, interp_line, interp_col, 1);

        // Push in reverse so they pop in correct order.
        self.pending.push(interp_end);
        self.pending.push(ident_token);
        self.pending.push(interp_start);

        if content.is_empty() {
            // No literal content before the interpolation -- return InterpolationStart directly.
            return Ok(self.pending.pop().unwrap());
        }

        let length = content.len() as u32;
        Ok(self.make_token(
            TokenKind::StringLiteral(content),
            start_line,
            start_col,
            length,
        ))
    }

    /// Handles `${expr}` inside a string.
    /// Emits: StringLiteral (if content), InterpolationStart, then switches to Interpolation mode.
    fn scan_expr_interpolation(
        &mut self,
        content: String,
        start_line: u32,
        start_col: u32,
    ) -> Result<Token, LexError> {
        let interp_line = self.line;
        let interp_col = self.column;

        // Consume `${`.
        self.advance(); // $
        self.advance(); // {

        // Push interpolation mode -- the next calls to next_token() will lex normally
        // until the matching `}` is found.
        self.push_mode(LexMode::Interpolation { brace_depth: 1 });

        let interp_start =
            self.make_token(TokenKind::InterpolationStart, interp_line, interp_col, 2);

        if content.is_empty() {
            return Ok(interp_start);
        }

        let length = content.len() as u32;
        self.pending.push(interp_start);
        Ok(self.make_token(
            TokenKind::StringLiteral(content),
            start_line,
            start_col,
            length,
        ))
    }

    // -------------------------------------------------------
    // Interpolation mode scanning
    // -------------------------------------------------------

    fn scan_interpolation(&mut self, brace_depth: u32) -> Result<Token, LexError> {
        self.skip_whitespace();

        if self.is_at_end() {
            return Err(self.error("unterminated string interpolation"));
        }

        let start_line = self.line;
        let start_col = self.column;

        // Check for closing `}` of the interpolation.
        if self.peek() == Some(b'}') {
            if brace_depth == 1 {
                self.advance();
                self.pop_mode();
                return Ok(self.make_token(TokenKind::InterpolationEnd, start_line, start_col, 1));
            }
            // Nested brace -- decrement depth and emit as normal token.
            self.advance();
            self.update_brace_depth(brace_depth - 1);
            return Ok(self.make_token(TokenKind::RightBrace, start_line, start_col, 1));
        }

        // Check for opening `{` -- increment depth.
        if self.peek() == Some(b'{') {
            self.advance();
            self.update_brace_depth(brace_depth + 1);
            return Ok(self.make_token(TokenKind::LeftBrace, start_line, start_col, 1));
        }

        // Otherwise, lex normally (reuse normal scanning).
        // Temporarily switch to Normal mode so scan_normal doesn't recurse into interpolation.
        let current_mode_idx = self.mode_stack.len() - 1;
        let saved_mode = self.mode_stack[current_mode_idx].clone();
        self.mode_stack[current_mode_idx] = LexMode::Normal;
        let result = self.scan_normal();
        self.mode_stack[current_mode_idx] = saved_mode;
        result
    }

    fn update_brace_depth(&mut self, new_depth: u32) {
        if let Some(LexMode::Interpolation { brace_depth }) = self.mode_stack.last_mut() {
            *brace_depth = new_depth;
        }
    }
}

/// Look up an identifier string and return the keyword `TokenKind` if it matches.
fn keyword_lookup(word: &str) -> Option<TokenKind> {
    match word {
        "class" => Some(TokenKind::Class),
        "trait" => Some(TokenKind::Trait),
        "enum" => Some(TokenKind::Enum),
        "struct" => Some(TokenKind::Struct),
        "func" => Some(TokenKind::Func),
        "let" => Some(TokenKind::Let),
        "var" => Some(TokenKind::Var),
        "const" => Some(TokenKind::Const),
        "public" => Some(TokenKind::Public),
        "private" => Some(TokenKind::Private),
        "static" => Some(TokenKind::Static),
        "extends" => Some(TokenKind::Extends),
        "with" => Some(TokenKind::With),
        "import" => Some(TokenKind::Import),
        "export" => Some(TokenKind::Export),
        "return" => Some(TokenKind::Return),
        "if" => Some(TokenKind::If),
        "else" => Some(TokenKind::Else),
        "when" => Some(TokenKind::When),
        "while" => Some(TokenKind::While),
        "for" => Some(TokenKind::For),
        "in" => Some(TokenKind::In),
        "break" => Some(TokenKind::Break),
        "continue" => Some(TokenKind::Continue),
        "is" => Some(TokenKind::Is),
        "as" => Some(TokenKind::As),
        "self" => Some(TokenKind::SelfKeyword),
        "super" => Some(TokenKind::Super),
        "where" => Some(TokenKind::Where),
        "start" => Some(TokenKind::Start),
        "yield" => Some(TokenKind::Yield),
        "true" => Some(TokenKind::True),
        "false" => Some(TokenKind::False),
        "null" => Some(TokenKind::Null),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_kinds(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        lexer
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_keyword_lookup_all() {
        let keywords = [
            ("class", TokenKind::Class),
            ("trait", TokenKind::Trait),
            ("enum", TokenKind::Enum),
            ("struct", TokenKind::Struct),
            ("func", TokenKind::Func),
            ("let", TokenKind::Let),
            ("var", TokenKind::Var),
            ("const", TokenKind::Const),
            ("public", TokenKind::Public),
            ("private", TokenKind::Private),
            ("static", TokenKind::Static),
            ("extends", TokenKind::Extends),
            ("with", TokenKind::With),
            ("import", TokenKind::Import),
            ("export", TokenKind::Export),
            ("return", TokenKind::Return),
            ("if", TokenKind::If),
            ("else", TokenKind::Else),
            ("when", TokenKind::When),
            ("while", TokenKind::While),
            ("for", TokenKind::For),
            ("in", TokenKind::In),
            ("break", TokenKind::Break),
            ("continue", TokenKind::Continue),
            ("is", TokenKind::Is),
            ("as", TokenKind::As),
            ("self", TokenKind::SelfKeyword),
            ("super", TokenKind::Super),
            ("where", TokenKind::Where),
            ("start", TokenKind::Start),
            ("yield", TokenKind::Yield),
            ("true", TokenKind::True),
            ("false", TokenKind::False),
        ];
        for (word, expected) in keywords {
            assert_eq!(
                keyword_lookup(word),
                Some(expected),
                "keyword_lookup failed for: {word}"
            );
        }
    }

    #[test]
    fn test_keyword_lookup_non_keyword() {
        assert_eq!(keyword_lookup("hello"), None);
        assert_eq!(keyword_lookup("from"), None);
        assert_eq!(keyword_lookup("x"), None);
    }

    #[test]
    fn test_peek_at_end() {
        let lexer = Lexer::new("");
        assert_eq!(lexer.peek(), None);
        assert_eq!(lexer.peek_ahead(0), None);
        assert_eq!(lexer.peek_ahead(1), None);
    }

    #[test]
    fn test_advance_tracks_position() {
        let mut lexer = Lexer::new("ab\nc");
        assert_eq!(lexer.advance(), Some(b'a'));
        assert_eq!(lexer.line, 1);
        assert_eq!(lexer.column, 2);

        assert_eq!(lexer.advance(), Some(b'b'));
        assert_eq!(lexer.line, 1);
        assert_eq!(lexer.column, 3);

        assert_eq!(lexer.advance(), Some(b'\n'));
        assert_eq!(lexer.line, 2);
        assert_eq!(lexer.column, 1);

        assert_eq!(lexer.advance(), Some(b'c'));
        assert_eq!(lexer.line, 2);
        assert_eq!(lexer.column, 2);

        assert_eq!(lexer.advance(), None);
    }

    #[test]
    fn test_number_before_range() {
        // `0..10` must lex as IntLiteral(0), DotDot, IntLiteral(10)
        let kinds = token_kinds("0..10");
        assert_eq!(
            kinds,
            vec![
                TokenKind::IntLiteral(0),
                TokenKind::DotDot,
                TokenKind::IntLiteral(10),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn test_escape_sequences() {
        let mut lexer = Lexer::new(r#""a\nb\tc\\d\"e\$f""#);
        let tokens = lexer.tokenize().unwrap();
        let literals: Vec<_> = tokens
            .iter()
            .filter_map(|t| {
                if let TokenKind::StringLiteral(s) = &t.kind {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(literals, vec!["a\nb\tc\\d\"e$f"]);
    }
}
