use crate::lexer::{Span, Token, TokenKind};

use super::ast::{
    ArrayElement, AssignOp, BinaryOp, CallArg, ClassDecl, Decl, DeclKind, DictElement, ElseBranch,
    EnumDecl, EnumVariant, Expr, ExprKind, FieldDecl, FuncDecl, FuncParam, ImportDecl,
    InterpolationSegment, LambdaBody, Literal, Setter, Stmt, StmtKind, StructDecl, TraitDecl,
    TraitMethod, TypeExpr, UnaryOp, Visibility, WhenArm, WhenBody, WhenPattern, WhereClause,
    WildcardImportDecl,
};
use super::error::ParseError;

/// Operator precedence levels, from lowest to highest binding power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum Precedence {
    Lowest = 0,
    Ternary = 1,
    Or = 2,
    And = 3,
    Equality = 4,
    Comparison = 5,
    Range = 6,
    NullCoalesce = 7,
    Addition = 8,
    Multiplication = 9,
    Cast = 10,
    Unary = 11,
    Postfix = 12,
}

/// Returns the infix precedence for a token, or `None` if it is not an infix operator.
fn infix_precedence(kind: &TokenKind) -> Option<Precedence> {
    match kind {
        TokenKind::Question => Some(Precedence::Ternary),
        TokenKind::PipePipe => Some(Precedence::Or),
        TokenKind::AmpAmp => Some(Precedence::And),
        TokenKind::EqualEqual | TokenKind::BangEqual => Some(Precedence::Equality),
        TokenKind::Less | TokenKind::Greater | TokenKind::LessEqual | TokenKind::GreaterEqual => {
            Some(Precedence::Comparison)
        }
        TokenKind::DotDot | TokenKind::DotDotEqual => Some(Precedence::Range),
        TokenKind::QuestionQuestion => Some(Precedence::NullCoalesce),
        TokenKind::Plus | TokenKind::Minus => Some(Precedence::Addition),
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Some(Precedence::Multiplication),
        TokenKind::As => Some(Precedence::Cast),
        TokenKind::Dot
        | TokenKind::QuestionDot
        | TokenKind::LeftParen
        | TokenKind::LeftBracket
        | TokenKind::ColonColon => Some(Precedence::Postfix),
        _ => None,
    }
}

/// Parses a token stream into an expression AST.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse a single expression.
    pub fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        self.parse_precedence(Precedence::Lowest)
    }

    // --- Token navigation ---

    fn peek(&self) -> &TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn peek_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|t| t.span.clone())
            .unwrap_or_else(|| Span {
                file: String::new(),
                line: 0,
                column: 0,
                length: 0,
            })
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens.get(self.pos).cloned().unwrap_or(Token {
            kind: TokenKind::Eof,
            span: Span {
                file: String::new(),
                line: 0,
                column: 0,
                length: 0,
            },
        });
        self.pos += 1;
        token
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        let token = self.advance();
        if std::mem::discriminant(&token.kind) == std::mem::discriminant(expected) {
            Ok(token)
        } else {
            Err(ParseError {
                message: format!("expected {expected:?}, found {:?}", token.kind),
                span: token.span,
            })
        }
    }

    fn expect_identifier(&mut self) -> Result<(String, Span), ParseError> {
        let token = self.advance();
        if let TokenKind::Identifier(name) = token.kind {
            Ok((name, token.span))
        } else {
            Err(ParseError {
                message: format!("expected identifier, found {:?}", token.kind),
                span: token.span,
            })
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == &TokenKind::Newline {
            self.pos += 1;
        }
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: self.peek_span(),
        }
    }

    // --- Type expression parsing ---

    /// Parses a type expression: `float`, `Result<float>`, `Array<Weapon>`, `(float, float)`.
    fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        if self.peek() == &TokenKind::LeftParen {
            // Tuple type: (float, float)
            self.advance(); // consume `(`
            let mut types = Vec::new();
            if self.peek() != &TokenKind::RightParen {
                types.push(self.parse_type_expr()?);
                while self.peek() == &TokenKind::Comma {
                    self.advance(); // consume `,`
                    types.push(self.parse_type_expr()?);
                }
            }
            self.expect(&TokenKind::RightParen)?;
            return Ok(TypeExpr::Tuple(types));
        }

        let (name, _) = self.expect_identifier()?;

        // Check for generic arguments: `<T>`, `<K, V>`
        if self.peek() == &TokenKind::Less {
            self.advance(); // consume `<`
            let mut args = Vec::new();
            args.push(self.parse_type_expr()?);
            while self.peek() == &TokenKind::Comma {
                self.advance(); // consume `,`
                args.push(self.parse_type_expr()?);
            }
            self.expect(&TokenKind::Greater)?;
            Ok(TypeExpr::Generic { name, args })
        } else {
            Ok(TypeExpr::Simple(name))
        }
    }

    // --- Shared parsing helpers ---

    /// Peeks at a token `offset` positions ahead (0 = current).
    fn peek_ahead(&self, offset: usize) -> &TokenKind {
        self.tokens
            .get(self.pos + offset)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    /// Consumes and returns a visibility modifier if present, else `Visibility::Default`.
    fn parse_visibility(&mut self) -> Visibility {
        match self.peek() {
            TokenKind::Public => {
                self.advance();
                Visibility::Public
            }
            TokenKind::Private => {
                self.advance();
                Visibility::Private
            }
            _ => Visibility::Default,
        }
    }

    /// Parses a function parameter list: `(name: Type, name: Type, ...name: Type)`.
    /// The opening `(` must already be consumed.
    fn parse_func_params(&mut self) -> Result<Vec<FuncParam>, ParseError> {
        let mut params = Vec::new();
        self.skip_newlines();
        if self.peek() == &TokenKind::RightParen {
            return Ok(params);
        }

        loop {
            self.skip_newlines();
            let is_variadic = if self.peek() == &TokenKind::DotDotDot {
                self.advance(); // consume `...`
                true
            } else {
                false
            };

            let (name, _) = self.expect_identifier()?;
            self.expect(&TokenKind::Colon)?;
            let type_annotation = self.parse_type_expr()?;

            params.push(FuncParam {
                name,
                type_annotation,
                is_variadic,
            });

            if self.peek() != &TokenKind::Comma {
                break;
            }
            self.advance(); // consume `,`
        }

        Ok(params)
    }

    /// Parses a single call argument, which may be named: `name: value` or positional: `value`.
    fn parse_call_arg(&mut self) -> Result<CallArg, ParseError> {
        self.skip_newlines();
        // Check for named argument pattern: `identifier: expr`
        if let TokenKind::Identifier(_) = self.peek()
            && self.peek_ahead(1) == &TokenKind::Colon
        {
            let (name, _) = self.expect_identifier()?;
            self.advance(); // consume `:`
            let value = self.parse_expr()?;
            return Ok(CallArg::Named { name, value });
        }
        let expr = self.parse_expr()?;
        Ok(CallArg::Positional(expr))
    }

    /// Expects an identifier with a specific string value (contextual keyword like `from`, `set`).
    fn expect_contextual(&mut self, word: &str) -> Result<Token, ParseError> {
        let token = self.advance();
        if let TokenKind::Identifier(ref name) = token.kind
            && name == word
        {
            return Ok(token);
        }
        Err(ParseError {
            message: format!("expected `{word}`, found {:?}", token.kind),
            span: token.span,
        })
    }

    /// Parses a plain string literal (consuming StringStart/StringLiteral/StringEnd tokens).
    /// Returns the string content.
    fn parse_plain_string(&mut self) -> Result<String, ParseError> {
        self.expect(&TokenKind::StringStart)?;
        let content = if let TokenKind::StringLiteral(s) = self.peek().clone() {
            self.advance();
            s
        } else {
            String::new()
        };
        self.expect(&TokenKind::StringEnd)?;
        Ok(content)
    }

    // --- Pratt parsing core ---

    fn parse_precedence(&mut self, min_prec: Precedence) -> Result<Expr, ParseError> {
        let mut left = self.parse_prefix()?;

        loop {
            self.skip_newlines();

            let Some(prec) = infix_precedence(self.peek()) else {
                break;
            };

            if prec <= min_prec {
                break;
            }

            left = self.parse_infix(left, prec)?;
        }

        Ok(left)
    }

    // --- Prefix parsing (atoms + unary) ---

    fn parse_prefix(&mut self) -> Result<Expr, ParseError> {
        self.skip_newlines();
        let kind = self.peek().clone();

        match kind {
            TokenKind::IntLiteral(n) => {
                let token = self.advance();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Int(n)),
                    span: token.span,
                })
            }

            TokenKind::FloatLiteral(n) => {
                let token = self.advance();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Float(n)),
                    span: token.span,
                })
            }

            TokenKind::True => {
                let token = self.advance();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Bool(true)),
                    span: token.span,
                })
            }

            TokenKind::False => {
                let token = self.advance();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Bool(false)),
                    span: token.span,
                })
            }

            TokenKind::Null => {
                let token = self.advance();
                Ok(Expr {
                    kind: ExprKind::Literal(Literal::Null),
                    span: token.span,
                })
            }

            TokenKind::Identifier(name) => {
                let token = self.advance();
                Ok(Expr {
                    kind: ExprKind::Identifier(name),
                    span: token.span,
                })
            }

            TokenKind::SelfKeyword => {
                let token = self.advance();
                Ok(Expr {
                    kind: ExprKind::Identifier("self".to_string()),
                    span: token.span,
                })
            }

            TokenKind::Super => {
                let span = self.advance().span; // consume `super`
                self.expect(&TokenKind::Dot)?;
                let (method, _) = self.expect_identifier()?;
                self.expect(&TokenKind::LeftParen)?;
                self.skip_newlines();
                let mut args = Vec::new();
                if self.peek() != &TokenKind::RightParen {
                    args.push(self.parse_call_arg()?);
                    while self.peek() == &TokenKind::Comma {
                        self.advance(); // consume `,`
                        self.skip_newlines();
                        args.push(self.parse_call_arg()?);
                    }
                }
                self.skip_newlines();
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr {
                    kind: ExprKind::Super { method, args },
                    span,
                })
            }

            TokenKind::StringStart | TokenKind::MultilineStringStart => {
                self.parse_string_expression()
            }

            TokenKind::LeftParen => self.parse_paren_expr(),

            TokenKind::Minus => {
                let token = self.advance();
                let operand = self.parse_precedence(Precedence::Unary)?;
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Negate,
                        operand: Box::new(operand),
                    },
                    span: token.span,
                })
            }

            TokenKind::Bang => {
                let token = self.advance();
                let operand = self.parse_precedence(Precedence::Unary)?;
                Ok(Expr {
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    span: token.span,
                })
            }

            TokenKind::LeftBracket => self.parse_array_literal(),

            TokenKind::LeftBrace => self.parse_dict_literal(),

            TokenKind::When => self.parse_when_expr(),

            TokenKind::Yield => {
                let token = self.advance();
                // Check if this is a bare yield (no argument)
                if matches!(
                    self.peek(),
                    TokenKind::Newline
                        | TokenKind::Semicolon
                        | TokenKind::RightParen
                        | TokenKind::RightBrace
                        | TokenKind::RightBracket
                        | TokenKind::Eof
                ) {
                    Ok(Expr {
                        kind: ExprKind::Yield(None),
                        span: token.span,
                    })
                } else {
                    let expr = self.parse_expr()?;
                    Ok(Expr {
                        kind: ExprKind::Yield(Some(Box::new(expr))),
                        span: token.span,
                    })
                }
            }

            _ => Err(self.error(format!("expected expression, found {kind:?}"))),
        }
    }

    // --- Infix parsing (binary, ternary, postfix) ---

    fn parse_infix(&mut self, left: Expr, prec: Precedence) -> Result<Expr, ParseError> {
        let op_token = self.advance();
        let span = left.span.clone();

        match &op_token.kind {
            // Standard binary operators
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::EqualEqual
            | TokenKind::BangEqual
            | TokenKind::Less
            | TokenKind::Greater
            | TokenKind::LessEqual
            | TokenKind::GreaterEqual
            | TokenKind::AmpAmp
            | TokenKind::PipePipe => {
                let op = token_to_binary_op(&op_token.kind);
                self.skip_newlines();
                let rhs = self.parse_precedence(prec)?;
                Ok(Expr {
                    kind: ExprKind::Binary {
                        op,
                        lhs: Box::new(left),
                        rhs: Box::new(rhs),
                    },
                    span,
                })
            }

            // Range: .. (exclusive)
            TokenKind::DotDot => {
                self.skip_newlines();
                let rhs = self.parse_precedence(prec)?;
                Ok(Expr {
                    kind: ExprKind::Range {
                        start: Box::new(left),
                        end: Box::new(rhs),
                        inclusive: false,
                    },
                    span,
                })
            }

            // Range: ..= (inclusive)
            TokenKind::DotDotEqual => {
                self.skip_newlines();
                let rhs = self.parse_precedence(prec)?;
                Ok(Expr {
                    kind: ExprKind::Range {
                        start: Box::new(left),
                        end: Box::new(rhs),
                        inclusive: true,
                    },
                    span,
                })
            }

            // Null coalesce: ??
            TokenKind::QuestionQuestion => {
                self.skip_newlines();
                let rhs = self.parse_precedence(prec)?;
                Ok(Expr {
                    kind: ExprKind::NullCoalesce {
                        lhs: Box::new(left),
                        rhs: Box::new(rhs),
                    },
                    span,
                })
            }

            // Postfix error propagation `expr?` vs ternary `condition ? then : else`.
            // If the token immediately after `?` terminates a statement, treat as
            // postfix error propagation. Otherwise parse as ternary.
            TokenKind::Question => {
                if matches!(
                    self.peek(),
                    TokenKind::Newline
                        | TokenKind::Semicolon
                        | TokenKind::RightParen
                        | TokenKind::RightBrace
                        | TokenKind::RightBracket
                        | TokenKind::Eof
                ) {
                    return Ok(Expr {
                        kind: ExprKind::ErrorPropagate(Box::new(left)),
                        span,
                    });
                }

                // Ternary: condition ? then_expr : else_expr
                self.skip_newlines();
                let then_expr = self.parse_precedence(Precedence::Lowest)?;
                self.skip_newlines();
                self.expect(&TokenKind::Colon)?;
                self.skip_newlines();
                // Right-associative: use one level below Ternary so nested ternaries
                // in the else branch associate rightward.
                let else_expr = self.parse_precedence(Precedence::Lowest)?;
                Ok(Expr {
                    kind: ExprKind::Ternary {
                        condition: Box::new(left),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    },
                    span,
                })
            }

            // Member access: .member
            TokenKind::Dot => {
                let (member, _) = self.expect_identifier()?;
                Ok(Expr {
                    kind: ExprKind::MemberAccess {
                        object: Box::new(left),
                        member,
                    },
                    span,
                })
            }

            // Safe access: ?.member
            TokenKind::QuestionDot => {
                let (member, _) = self.expect_identifier()?;
                Ok(Expr {
                    kind: ExprKind::SafeAccess {
                        object: Box::new(left),
                        member,
                    },
                    span,
                })
            }

            // Cast: expr as Type
            TokenKind::As => {
                let target_type = self.parse_type_expr()?;
                Ok(Expr {
                    kind: ExprKind::Cast {
                        expr: Box::new(left),
                        target_type,
                    },
                    span,
                })
            }

            // Call: callee(arg1, name: arg2, ...)
            TokenKind::LeftParen => {
                self.skip_newlines();
                let mut args = Vec::new();
                if self.peek() != &TokenKind::RightParen {
                    args.push(self.parse_call_arg()?);
                    while self.peek() == &TokenKind::Comma {
                        self.advance(); // consume `,`
                        self.skip_newlines();
                        args.push(self.parse_call_arg()?);
                    }
                }
                self.skip_newlines();
                self.expect(&TokenKind::RightParen)?;
                Ok(Expr {
                    kind: ExprKind::Call {
                        callee: Box::new(left),
                        args,
                    },
                    span,
                })
            }

            // Index: collection[index]
            TokenKind::LeftBracket => {
                self.skip_newlines();
                let index = self.parse_expr()?;
                self.skip_newlines();
                self.expect(&TokenKind::RightBracket)?;
                Ok(Expr {
                    kind: ExprKind::Index {
                        object: Box::new(left),
                        index: Box::new(index),
                    },
                    span,
                })
            }

            // Namespace access: alias::Member
            TokenKind::ColonColon => {
                let namespace = match left.kind {
                    ExprKind::Identifier(ref name) => name.clone(),
                    _ => {
                        return Err(ParseError {
                            message: "namespace access requires an identifier on the left"
                                .to_string(),
                            span,
                        });
                    }
                };
                let (member, _) = self.expect_identifier()?;
                Ok(Expr {
                    kind: ExprKind::NamespaceAccess { namespace, member },
                    span,
                })
            }

            _ => Err(ParseError {
                message: format!("unexpected infix operator {:?}", op_token.kind),
                span: op_token.span,
            }),
        }
    }

    // --- String interpolation ---

    fn parse_string_expression(&mut self) -> Result<Expr, ParseError> {
        let start_token = self.advance();
        let span = start_token.span.clone();
        let mut segments: Vec<InterpolationSegment> = Vec::new();
        let mut has_interpolation = false;

        let end_kind = match &start_token.kind {
            TokenKind::MultilineStringStart => TokenKind::MultilineStringEnd,
            _ => TokenKind::StringEnd,
        };

        loop {
            match self.peek().clone() {
                TokenKind::StringLiteral(s) => {
                    self.advance();
                    segments.push(InterpolationSegment::Literal(s));
                }

                TokenKind::InterpolationStart => {
                    has_interpolation = true;
                    self.advance();
                    let expr = self.parse_expr()?;
                    segments.push(InterpolationSegment::Expression(expr));
                    self.expect(&TokenKind::InterpolationEnd)?;
                }

                ref k if std::mem::discriminant(k) == std::mem::discriminant(&end_kind) => {
                    self.advance();
                    break;
                }

                TokenKind::Eof => {
                    return Err(self.error("unterminated string"));
                }

                other => {
                    return Err(self.error(format!("unexpected token in string: {other:?}")));
                }
            }
        }

        // Plain string with no interpolation -> Literal
        if !has_interpolation {
            return match segments.len() {
                0 => Ok(Expr {
                    kind: ExprKind::Literal(Literal::String(String::new())),
                    span,
                }),
                1 => {
                    let segment = segments.into_iter().next().unwrap();
                    if let InterpolationSegment::Literal(s) = segment {
                        Ok(Expr {
                            kind: ExprKind::Literal(Literal::String(s)),
                            span,
                        })
                    } else {
                        unreachable!("no interpolation but segment is expression")
                    }
                }
                _ => Ok(Expr {
                    kind: ExprKind::StringInterpolation(segments),
                    span,
                }),
            };
        }

        Ok(Expr {
            kind: ExprKind::StringInterpolation(segments),
            span,
        })
    }

    // --- Statement termination helpers ---

    /// Returns true if the current token terminates a statement.
    fn at_stmt_terminator(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::RightBrace | TokenKind::Eof
        )
    }

    /// Consumes one semicolon or newline if present.
    fn consume_stmt_terminator(&mut self) {
        if matches!(self.peek(), TokenKind::Semicolon | TokenKind::Newline) {
            self.advance();
        }
    }

    /// Returns true if the current token is an assignment operator.
    fn is_assignment_op(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Assign
                | TokenKind::PlusAssign
                | TokenKind::MinusAssign
                | TokenKind::StarAssign
                | TokenKind::SlashAssign
                | TokenKind::PercentAssign
        )
    }

    /// Consumes an assignment operator token and returns the corresponding `AssignOp`.
    fn parse_assignment_op(&mut self) -> AssignOp {
        let token = self.advance();
        match token.kind {
            TokenKind::Assign => AssignOp::Assign,
            TokenKind::PlusAssign => AssignOp::AddAssign,
            TokenKind::MinusAssign => AssignOp::SubAssign,
            TokenKind::StarAssign => AssignOp::MulAssign,
            TokenKind::SlashAssign => AssignOp::DivAssign,
            TokenKind::PercentAssign => AssignOp::ModAssign,
            _ => unreachable!("not an assignment operator: {:?}", token.kind),
        }
    }

    // --- Block parsing ---

    /// Parses a block: `{ stmt* }`. Returns the inner statements.
    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();
        let mut stmts = Vec::new();
        while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        self.expect(&TokenKind::RightBrace)?;
        Ok(stmts)
    }

    // --- Statement parsing ---

    /// Parse a single statement.
    pub fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();
        let span = self.peek_span();

        match self.peek().clone() {
            TokenKind::Let => self.parse_let_decl(),
            TokenKind::Var => self.parse_var_decl(),
            TokenKind::Const => self.parse_const_decl(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Break => {
                self.advance();
                self.consume_stmt_terminator();
                Ok(Stmt {
                    kind: StmtKind::Break,
                    span,
                })
            }
            TokenKind::Continue => {
                self.advance();
                self.consume_stmt_terminator();
                Ok(Stmt {
                    kind: StmtKind::Continue,
                    span,
                })
            }
            TokenKind::If => self.parse_if_stmt(),
            TokenKind::While => self.parse_while_stmt(),
            TokenKind::For => self.parse_for_stmt(),
            TokenKind::When => self.parse_when_stmt(),
            TokenKind::Func => {
                let func = self.parse_func_decl(false, Visibility::Default)?;
                Ok(Stmt {
                    kind: StmtKind::Func(func),
                    span,
                })
            }
            TokenKind::Static if self.peek_ahead(1) == &TokenKind::Func => {
                self.advance(); // consume `static`
                let func = self.parse_func_decl(true, Visibility::Default)?;
                Ok(Stmt {
                    kind: StmtKind::Func(func),
                    span,
                })
            }
            TokenKind::Public | TokenKind::Private
                if matches!(
                    self.peek_ahead(1),
                    TokenKind::Class
                        | TokenKind::Trait
                        | TokenKind::Enum
                        | TokenKind::Struct
                        | TokenKind::Func
                ) =>
            {
                let _vis = self.parse_visibility();
                match self.peek().clone() {
                    TokenKind::Class => Ok(Stmt {
                        kind: StmtKind::Class(self.parse_class_decl()?),
                        span,
                    }),
                    TokenKind::Trait => Ok(Stmt {
                        kind: StmtKind::Trait(self.parse_trait_decl()?),
                        span,
                    }),
                    TokenKind::Enum => Ok(Stmt {
                        kind: StmtKind::Enum(self.parse_enum_decl()?),
                        span,
                    }),
                    TokenKind::Struct => Ok(Stmt {
                        kind: StmtKind::Struct(self.parse_struct_decl()?),
                        span,
                    }),
                    TokenKind::Func => {
                        let func = self.parse_func_decl(false, _vis)?;
                        Ok(Stmt {
                            kind: StmtKind::Func(func),
                            span,
                        })
                    }
                    _ => unreachable!(),
                }
            }
            TokenKind::Class => {
                let class = self.parse_class_decl()?;
                Ok(Stmt {
                    kind: StmtKind::Class(class),
                    span,
                })
            }
            TokenKind::Trait => {
                let trait_decl = self.parse_trait_decl()?;
                Ok(Stmt {
                    kind: StmtKind::Trait(trait_decl),
                    span,
                })
            }
            TokenKind::Enum => {
                let enum_decl = self.parse_enum_decl()?;
                Ok(Stmt {
                    kind: StmtKind::Enum(enum_decl),
                    span,
                })
            }
            TokenKind::Struct => {
                let struct_decl = self.parse_struct_decl()?;
                Ok(Stmt {
                    kind: StmtKind::Struct(struct_decl),
                    span,
                })
            }
            TokenKind::Import => {
                let decl = self.parse_import_decl()?;
                match decl.kind {
                    DeclKind::Import(import) => Ok(Stmt {
                        kind: StmtKind::Import(import),
                        span,
                    }),
                    DeclKind::WildcardImport(wildcard) => Ok(Stmt {
                        kind: StmtKind::WildcardImport(wildcard),
                        span,
                    }),
                    _ => unreachable!(),
                }
            }
            TokenKind::Export => {
                let span = self.advance().span; // consume `export`
                let inner = self.parse_stmt()?;
                Ok(Stmt {
                    kind: StmtKind::Export(Box::new(inner)),
                    span,
                })
            }
            TokenKind::Start => {
                let span = self.advance().span; // consume `start`
                let expr = self.parse_expr()?;
                self.consume_stmt_terminator();
                Ok(Stmt {
                    kind: StmtKind::Start(expr),
                    span,
                })
            }
            TokenKind::LeftBrace => {
                let block = self.parse_block()?;
                Ok(Stmt {
                    kind: StmtKind::Block(block),
                    span,
                })
            }
            _ => {
                let expr = self.parse_expr()?;
                if self.is_assignment_op() {
                    let op = self.parse_assignment_op();
                    let value = self.parse_expr()?;
                    self.consume_stmt_terminator();
                    Ok(Stmt {
                        kind: StmtKind::Assignment {
                            target: expr,
                            op,
                            value,
                        },
                        span,
                    })
                } else {
                    self.consume_stmt_terminator();
                    Ok(Stmt {
                        kind: StmtKind::ExprStmt(expr),
                        span,
                    })
                }
            }
        }
    }

    /// Parse a sequence of statements until EOF.
    pub fn parse_program(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        self.skip_newlines();
        while self.peek() != &TokenKind::Eof {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }
        Ok(stmts)
    }

    // --- Variable declarations ---

    /// Parses `let name [: type] = expr` or `let (x, y) = expr`.
    fn parse_let_decl(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `let`

        // Tuple destructuring: `let (x, y) = expr`
        if self.peek() == &TokenKind::LeftParen {
            self.advance(); // consume `(`
            let mut names = Vec::new();
            self.skip_newlines();
            if self.peek() != &TokenKind::RightParen {
                let (first, _) = self.expect_identifier()?;
                names.push(first);
                while self.peek() == &TokenKind::Comma {
                    self.advance(); // consume `,`
                    self.skip_newlines();
                    let (name, _) = self.expect_identifier()?;
                    names.push(name);
                }
            }
            self.skip_newlines();
            self.expect(&TokenKind::RightParen)?;
            self.expect(&TokenKind::Assign)?;
            let initializer = self.parse_expr()?;
            self.consume_stmt_terminator();
            return Ok(Stmt {
                kind: StmtKind::LetDestructure { names, initializer },
                span,
            });
        }

        let (name, _) = self.expect_identifier()?;

        let type_annotation = if self.peek() == &TokenKind::Colon {
            self.advance(); // consume `:`
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        self.expect(&TokenKind::Assign)?;
        let initializer = self.parse_expr()?;
        self.consume_stmt_terminator();

        Ok(Stmt {
            kind: StmtKind::Let {
                name,
                type_annotation,
                initializer,
            },
            span,
        })
    }

    /// Parses `var name [: type] = expr`.
    fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `var`
        let (name, _) = self.expect_identifier()?;

        let type_annotation = if self.peek() == &TokenKind::Colon {
            self.advance(); // consume `:`
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        self.expect(&TokenKind::Assign)?;
        let initializer = self.parse_expr()?;
        self.consume_stmt_terminator();

        Ok(Stmt {
            kind: StmtKind::Var {
                name,
                type_annotation,
                initializer,
            },
            span,
        })
    }

    /// Parses `const name = expr`.
    fn parse_const_decl(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `const`
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Assign)?;
        let initializer = self.parse_expr()?;
        self.consume_stmt_terminator();

        Ok(Stmt {
            kind: StmtKind::Const { name, initializer },
            span,
        })
    }

    // --- Return ---

    /// Parses `return [expr]`.
    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `return`

        let value = if self.at_stmt_terminator() {
            None
        } else {
            Some(self.parse_expr()?)
        };

        self.consume_stmt_terminator();
        Ok(Stmt {
            kind: StmtKind::Return(value),
            span,
        })
    }

    // --- If/else ---

    /// Parses `if condition { ... } [else if ... | else { ... }]`.
    fn parse_if_stmt(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `if`
        let condition = self.parse_expr()?;
        let then_block = self.parse_block()?;

        self.skip_newlines();
        let else_branch = if self.peek() == &TokenKind::Else {
            self.advance(); // consume `else`
            self.skip_newlines();
            if self.peek() == &TokenKind::If {
                let if_stmt = self.parse_if_stmt()?;
                Some(ElseBranch::ElseIf(Box::new(if_stmt)))
            } else {
                let block = self.parse_block()?;
                Some(ElseBranch::ElseBlock(block))
            }
        } else {
            None
        };

        Ok(Stmt {
            kind: StmtKind::If {
                condition,
                then_block,
                else_branch,
            },
            span,
        })
    }

    // --- Loops ---

    /// Parses `while condition { body }`.
    fn parse_while_stmt(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `while`
        let condition = self.parse_expr()?;
        let body = self.parse_block()?;

        Ok(Stmt {
            kind: StmtKind::While { condition, body },
            span,
        })
    }

    /// Parses `for variable in iterable { body }`.
    fn parse_for_stmt(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `for`
        let (variable, _) = self.expect_identifier()?;
        self.expect(&TokenKind::In)?;
        let iterable = self.parse_expr()?;
        let body = self.parse_block()?;

        Ok(Stmt {
            kind: StmtKind::For {
                variable,
                iterable,
                body,
            },
            span,
        })
    }

    // --- When statement ---

    /// Parses `when [subject] { arms }`.
    fn parse_when_stmt(&mut self) -> Result<Stmt, ParseError> {
        let span = self.advance().span; // consume `when`
        self.skip_newlines();

        let subject = if self.peek() == &TokenKind::LeftBrace {
            None
        } else {
            Some(self.parse_expr()?)
        };

        self.skip_newlines();
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
            arms.push(self.parse_when_arm()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(Stmt {
            kind: StmtKind::When { subject, arms },
            span,
        })
    }

    /// Parses a `when` expression (used in expression position).
    /// Same syntax as the statement form, but produces `ExprKind::When`.
    fn parse_when_expr(&mut self) -> Result<Expr, ParseError> {
        let span = self.advance().span; // consume `when`
        self.skip_newlines();

        let subject = if self.peek() == &TokenKind::LeftBrace {
            None
        } else {
            Some(Box::new(self.parse_expr()?))
        };

        self.skip_newlines();
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
            arms.push(self.parse_when_arm()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(Expr {
            kind: ExprKind::When { subject, arms },
            span,
        })
    }

    /// Parses a single `when` arm: `pattern => body`.
    fn parse_when_arm(&mut self) -> Result<WhenArm, ParseError> {
        self.skip_newlines();
        let pattern = self.parse_when_pattern()?;
        self.skip_newlines();
        self.expect(&TokenKind::FatArrow)?;
        self.skip_newlines();

        let body = if self.peek() == &TokenKind::LeftBrace {
            WhenBody::Block(self.parse_block()?)
        } else {
            WhenBody::Expr(self.parse_expr()?)
        };

        self.consume_stmt_terminator();

        Ok(WhenArm { pattern, body })
    }

    // --- Declaration parsing (Phase 4) ---

    /// Parse a single top-level declaration.
    pub fn parse_decl(&mut self) -> Result<Decl, ParseError> {
        self.skip_newlines();
        let span = self.peek_span();

        match self.peek().clone() {
            TokenKind::Func => {
                let func = self.parse_func_decl(false, Visibility::Default)?;
                Ok(Decl {
                    kind: DeclKind::Func(func),
                    span,
                })
            }
            TokenKind::Static => {
                if self.peek_ahead(1) == &TokenKind::Func {
                    self.advance(); // consume `static`
                    let func = self.parse_func_decl(true, Visibility::Default)?;
                    Ok(Decl {
                        kind: DeclKind::Func(func),
                        span,
                    })
                } else {
                    // Fall through to statement parsing
                    let stmt = self.parse_stmt()?;
                    Ok(Decl {
                        kind: DeclKind::Stmt(stmt),
                        span,
                    })
                }
            }
            TokenKind::Class => {
                let class = self.parse_class_decl()?;
                Ok(Decl {
                    kind: DeclKind::Class(class),
                    span,
                })
            }
            TokenKind::Trait => {
                let trait_decl = self.parse_trait_decl()?;
                Ok(Decl {
                    kind: DeclKind::Trait(trait_decl),
                    span,
                })
            }
            TokenKind::Enum => {
                let enum_decl = self.parse_enum_decl()?;
                Ok(Decl {
                    kind: DeclKind::Enum(enum_decl),
                    span,
                })
            }
            TokenKind::Struct => {
                let struct_decl = self.parse_struct_decl()?;
                Ok(Decl {
                    kind: DeclKind::Struct(struct_decl),
                    span,
                })
            }
            TokenKind::Import => {
                let import = self.parse_import_decl()?;
                Ok(import)
            }
            TokenKind::Export => {
                let export = self.parse_export_decl()?;
                Ok(export)
            }
            _ => {
                let stmt = self.parse_stmt()?;
                Ok(Decl {
                    kind: DeclKind::Stmt(stmt),
                    span,
                })
            }
        }
    }

    /// Parse a complete file as a sequence of declarations.
    pub fn parse_file(&mut self) -> Result<Vec<Decl>, ParseError> {
        let mut decls = Vec::new();
        self.skip_newlines();
        while self.peek() != &TokenKind::Eof {
            decls.push(self.parse_decl()?);
            self.skip_newlines();
        }
        Ok(decls)
    }

    // --- Function declarations ---

    /// Parses optional generic type parameters `<T, U>` after a declaration name.
    /// Returns an empty vec if no `<` follows.
    fn parse_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        if self.peek() != &TokenKind::Less {
            return Ok(Vec::new());
        }
        self.advance(); // consume `<`
        let mut params = Vec::new();
        let (first, _) = self.expect_identifier()?;
        params.push(first);
        while self.peek() == &TokenKind::Comma {
            self.advance(); // consume `,`
            let (param, _) = self.expect_identifier()?;
            params.push(param);
        }
        self.expect(&TokenKind::Greater)?;
        Ok(params)
    }

    /// Parses optional `where T : Trait, U : OtherTrait` clause.
    /// Returns an empty vec if `where` does not follow.
    fn parse_where_clauses(&mut self) -> Result<Vec<WhereClause>, ParseError> {
        if self.peek() != &TokenKind::Where {
            return Ok(Vec::new());
        }
        self.advance(); // consume `where`
        let mut clauses = Vec::new();
        loop {
            let (type_param, _) = self.expect_identifier()?;
            self.expect(&TokenKind::Colon)?;
            let (trait_name, _) = self.expect_identifier()?;
            clauses.push(WhereClause {
                type_param,
                trait_name,
            });
            if self.peek() == &TokenKind::Comma {
                self.advance(); // consume `,`
                self.skip_newlines();
            } else {
                break;
            }
        }
        Ok(clauses)
    }

    /// Parses `func name[<T>](params) [-> Type] [where T : Trait] { body }`.
    fn parse_func_decl(
        &mut self,
        is_static: bool,
        visibility: Visibility,
    ) -> Result<FuncDecl, ParseError> {
        self.expect(&TokenKind::Func)?; // consume `func`
        let (name, _) = self.expect_identifier()?;
        let type_params = self.parse_type_params()?;
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_func_params()?;
        self.expect(&TokenKind::RightParen)?;

        let return_type = if self.peek() == &TokenKind::Arrow {
            self.advance(); // consume `->`
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        let where_clauses = self.parse_where_clauses()?;
        let body = self.parse_block()?;

        Ok(FuncDecl {
            name,
            type_params,
            params,
            return_type,
            body,
            is_static,
            visibility,
            where_clauses,
        })
    }

    // --- Lambda / Tuple / Grouped disambiguation ---

    /// Parses `(` ... `)` which could be a lambda, tuple, or grouped expression.
    fn parse_paren_expr(&mut self) -> Result<Expr, ParseError> {
        let open = self.advance(); // consume `(`
        let span = open.span.clone();
        self.skip_newlines();

        // Empty parens: `()` -- check for lambda `() => ...`
        if self.peek() == &TokenKind::RightParen {
            self.advance(); // consume `)`
            if self.peek() == &TokenKind::FatArrow {
                self.advance(); // consume `=>`
                self.skip_newlines();
                let body = if self.peek() == &TokenKind::LeftBrace {
                    LambdaBody::Block(self.parse_block()?)
                } else {
                    LambdaBody::Expr(Box::new(self.parse_expr()?))
                };
                return Ok(Expr {
                    kind: ExprKind::Lambda {
                        params: Vec::new(),
                        body,
                    },
                    span,
                });
            }
            // Empty tuple
            return Ok(Expr {
                kind: ExprKind::Tuple(Vec::new()),
                span,
            });
        }

        // Try lambda: save position, attempt to parse params
        let saved_pos = self.pos;
        if let Some(params) = self.try_parse_lambda_params() {
            if self.peek() == &TokenKind::RightParen {
                self.advance(); // consume `)`
                if self.peek() == &TokenKind::FatArrow {
                    self.advance(); // consume `=>`
                    self.skip_newlines();
                    let body = if self.peek() == &TokenKind::LeftBrace {
                        LambdaBody::Block(self.parse_block()?)
                    } else {
                        LambdaBody::Expr(Box::new(self.parse_expr()?))
                    };
                    return Ok(Expr {
                        kind: ExprKind::Lambda { params, body },
                        span,
                    });
                }
            }
            // Not a lambda -- restore position
            self.pos = saved_pos;
        }

        // Parse as expression(s)
        let first = self.parse_expr()?;
        self.skip_newlines();

        if self.peek() == &TokenKind::Comma {
            // Tuple: (expr, expr, ...)
            let mut elements = vec![first];
            while self.peek() == &TokenKind::Comma {
                self.advance(); // consume `,`
                self.skip_newlines();
                if self.peek() == &TokenKind::RightParen {
                    break; // trailing comma
                }
                elements.push(self.parse_expr()?);
                self.skip_newlines();
            }
            self.expect(&TokenKind::RightParen)?;
            Ok(Expr {
                kind: ExprKind::Tuple(elements),
                span,
            })
        } else {
            // Grouped: (expr)
            self.expect(&TokenKind::RightParen)?;
            Ok(Expr {
                kind: ExprKind::Grouped(Box::new(first)),
                span,
            })
        }
    }

    /// Parses an array literal: `[expr, ...spread, expr]`.
    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        let open = self.advance(); // consume `[`
        let span = open.span.clone();
        self.skip_newlines();

        let mut elements = Vec::new();
        if self.peek() != &TokenKind::RightBracket {
            elements.push(self.parse_array_element()?);
            while self.peek() == &TokenKind::Comma {
                self.advance(); // consume `,`
                self.skip_newlines();
                if self.peek() == &TokenKind::RightBracket {
                    break; // trailing comma
                }
                elements.push(self.parse_array_element()?);
            }
        }
        self.skip_newlines();
        self.expect(&TokenKind::RightBracket)?;
        Ok(Expr {
            kind: ExprKind::ArrayLiteral(elements),
            span,
        })
    }

    /// Parses a single array element -- either `...expr` (spread) or `expr`.
    fn parse_array_element(&mut self) -> Result<ArrayElement, ParseError> {
        self.skip_newlines();
        if self.peek() == &TokenKind::DotDotDot {
            self.advance(); // consume `...`
            let expr = self.parse_precedence(Precedence::NullCoalesce)?;
            Ok(ArrayElement::Spread(expr))
        } else {
            let expr = self.parse_expr()?;
            Ok(ArrayElement::Expr(expr))
        }
    }

    /// Parses a dictionary literal: `{key: value, ...spread}`.
    fn parse_dict_literal(&mut self) -> Result<Expr, ParseError> {
        let open = self.advance(); // consume `{`
        let span = open.span.clone();
        self.skip_newlines();

        let mut entries = Vec::new();
        if self.peek() != &TokenKind::RightBrace {
            entries.push(self.parse_dict_element()?);
            while self.peek() == &TokenKind::Comma {
                self.advance(); // consume `,`
                self.skip_newlines();
                if self.peek() == &TokenKind::RightBrace {
                    break; // trailing comma
                }
                entries.push(self.parse_dict_element()?);
            }
        }
        self.skip_newlines();
        self.expect(&TokenKind::RightBrace)?;
        Ok(Expr {
            kind: ExprKind::DictLiteral(entries),
            span,
        })
    }

    /// Parses a single dict element -- either `...expr` (spread) or `key: value`.
    fn parse_dict_element(&mut self) -> Result<DictElement, ParseError> {
        self.skip_newlines();
        if self.peek() == &TokenKind::DotDotDot {
            self.advance(); // consume `...`
            let expr = self.parse_precedence(Precedence::NullCoalesce)?;
            Ok(DictElement::Spread(expr))
        } else {
            let key = self.parse_expr()?;
            self.expect(&TokenKind::Colon)?;
            self.skip_newlines();
            let value = self.parse_expr()?;
            Ok(DictElement::KeyValue { key, value })
        }
    }

    /// Attempts to parse lambda parameters. Returns `None` and restores position on failure.
    fn try_parse_lambda_params(&mut self) -> Option<Vec<FuncParam>> {
        let saved_pos = self.pos;
        let mut params = Vec::new();

        loop {
            self.skip_newlines();

            let is_variadic = if self.peek() == &TokenKind::DotDotDot {
                self.advance();
                true
            } else {
                false
            };

            // Expect identifier
            let name = match self.peek() {
                TokenKind::Identifier(_) => {
                    if let TokenKind::Identifier(n) = self.advance().kind {
                        n
                    } else {
                        unreachable!()
                    }
                }
                _ => {
                    self.pos = saved_pos;
                    return None;
                }
            };

            // Expect `:`
            if self.peek() != &TokenKind::Colon {
                self.pos = saved_pos;
                return None;
            }
            self.advance();

            // Parse type expression
            let type_annotation = match self.parse_type_expr() {
                Ok(t) => t,
                Err(_) => {
                    self.pos = saved_pos;
                    return None;
                }
            };

            params.push(FuncParam {
                name,
                type_annotation,
                is_variadic,
            });

            if self.peek() != &TokenKind::Comma {
                break;
            }
            self.advance(); // consume `,`
        }

        Some(params)
    }

    // --- Class declarations ---

    /// Parses `class Name[<T, U>] [extends Parent] [with Trait1, Trait2] [where T : Trait] { body }`.
    fn parse_class_decl(&mut self) -> Result<ClassDecl, ParseError> {
        self.expect(&TokenKind::Class)?; // consume `class`
        let (name, _) = self.expect_identifier()?;
        let type_params = self.parse_type_params()?;

        // Optional `extends Parent`
        let extends = if self.peek() == &TokenKind::Extends {
            self.advance();
            let (parent, _) = self.expect_identifier()?;
            Some(parent)
        } else {
            None
        };

        // Optional `with Trait1, Trait2`
        let mut traits = Vec::new();
        if self.peek() == &TokenKind::With {
            self.advance();
            let (first_trait, _) = self.expect_identifier()?;
            traits.push(first_trait);
            while self.peek() == &TokenKind::Comma {
                self.advance(); // consume `,`
                let (trait_name, _) = self.expect_identifier()?;
                traits.push(trait_name);
            }
        }

        // Optional `where T : Trait, U : OtherTrait`
        let where_clauses = self.parse_where_clauses()?;

        // Parse body: { fields and methods }
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
            self.skip_newlines();
            let vis = self.parse_visibility();

            if self.peek() == &TokenKind::Static || self.peek() == &TokenKind::Func {
                // Method
                let is_static = if self.peek() == &TokenKind::Static {
                    self.advance();
                    true
                } else {
                    false
                };
                let method = self.parse_func_decl(is_static, vis)?;
                methods.push(method);
            } else if let TokenKind::Identifier(_) = self.peek() {
                // Field
                let field = self.parse_field_decl(vis)?;
                fields.push(field);
            } else {
                return Err(self.error(format!(
                    "expected field or method in class body, found {:?}",
                    self.peek()
                )));
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(ClassDecl {
            name,
            type_params,
            extends,
            traits,
            fields,
            methods,
            where_clauses,
        })
    }

    /// Parses a struct declaration: `struct Name[<T>] { fields; methods }`.
    /// Structs are value types -- no `extends`, no `with`.
    fn parse_struct_decl(&mut self) -> Result<StructDecl, ParseError> {
        self.expect(&TokenKind::Struct)?; // consume `struct`
        let (name, _) = self.expect_identifier()?;
        let type_params = self.parse_type_params()?;

        // Structs do not support extends or with
        if self.peek() == &TokenKind::Extends {
            return Err(self.error(
                "structs cannot use 'extends' -- structs do not support inheritance".to_string(),
            ));
        }
        if self.peek() == &TokenKind::With {
            return Err(self
                .error("structs cannot use 'with' -- structs do not support traits".to_string()));
        }

        // Parse body: { fields and methods }
        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
            self.skip_newlines();
            let vis = self.parse_visibility();

            if self.peek() == &TokenKind::Static || self.peek() == &TokenKind::Func {
                // Method
                let is_static = if self.peek() == &TokenKind::Static {
                    self.advance();
                    true
                } else {
                    false
                };
                let method = self.parse_func_decl(is_static, vis)?;
                methods.push(method);
            } else if let TokenKind::Identifier(_) = self.peek() {
                // Field
                let field = self.parse_field_decl(vis)?;
                fields.push(field);
            } else {
                return Err(self.error(format!(
                    "expected field or method in struct body, found {:?}",
                    self.peek()
                )));
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(StructDecl {
            name,
            type_params,
            fields,
            methods,
        })
    }

    /// Parses a field declaration: `name: Type [= default] [\n set(param) { body }]`.
    fn parse_field_decl(&mut self, visibility: Visibility) -> Result<FieldDecl, ParseError> {
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        let type_annotation = self.parse_type_expr()?;

        // Optional default value
        let default = if self.peek() == &TokenKind::Assign {
            self.advance(); // consume `=`
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.consume_stmt_terminator();
        self.skip_newlines();

        // Optional setter: `set(param) { body }`
        let setter = if let TokenKind::Identifier(ref s) = self.peek().clone() {
            if s == "set" && self.peek_ahead(1) == &TokenKind::LeftParen {
                Some(self.parse_setter()?)
            } else {
                None
            }
        } else {
            None
        };

        Ok(FieldDecl {
            name,
            type_annotation,
            default,
            visibility,
            setter,
        })
    }

    /// Parses `set(param) { body }`.
    fn parse_setter(&mut self) -> Result<Setter, ParseError> {
        self.expect_contextual("set")?;
        self.expect(&TokenKind::LeftParen)?;
        let (param_name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::RightParen)?;
        let body = self.parse_block()?;
        Ok(Setter { param_name, body })
    }

    // --- Trait declarations ---

    /// Parses `trait Name { methods }`.
    fn parse_trait_decl(&mut self) -> Result<TraitDecl, ParseError> {
        self.expect(&TokenKind::Trait)?; // consume `trait`
        let (name, _) = self.expect_identifier()?;

        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
            methods.push(self.parse_trait_method()?);
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(TraitDecl { name, methods })
    }

    /// Parses a trait method: `func name(params) [-> Type] [{ default_body }]`.
    fn parse_trait_method(&mut self) -> Result<TraitMethod, ParseError> {
        self.expect(&TokenKind::Func)?;
        let (name, _) = self.expect_identifier()?;
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_func_params()?;
        self.expect(&TokenKind::RightParen)?;

        let return_type = if self.peek() == &TokenKind::Arrow {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        // Optional default body
        self.skip_newlines();
        let default_body = if self.peek() == &TokenKind::LeftBrace {
            Some(self.parse_block()?)
        } else {
            self.consume_stmt_terminator();
            None
        };

        Ok(TraitMethod {
            name,
            params,
            return_type,
            default_body,
        })
    }

    // --- Enum declarations ---

    /// Parses `enum Name { variants [; fields; methods] }`.
    fn parse_enum_decl(&mut self) -> Result<EnumDecl, ParseError> {
        self.expect(&TokenKind::Enum)?; // consume `enum`
        let (name, _) = self.expect_identifier()?;

        self.expect(&TokenKind::LeftBrace)?;
        self.skip_newlines();

        // Parse variants (comma-separated identifiers, optionally with values)
        let mut variants = Vec::new();
        while let TokenKind::Identifier(_) = self.peek() {
            // Check if this looks like a field (identifier followed by `:`) rather than a variant
            if self.peek_ahead(1) == &TokenKind::Colon {
                break;
            }
            let (variant_name, _) = self.expect_identifier()?;
            let value = if self.peek() == &TokenKind::LeftParen {
                self.advance(); // consume `(`
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RightParen)?;
                Some(expr)
            } else {
                None
            };
            variants.push(EnumVariant {
                name: variant_name,
                value,
            });
            if self.peek() == &TokenKind::Comma {
                self.advance(); // consume `,`
                self.skip_newlines();
            } else {
                self.consume_stmt_terminator();
                self.skip_newlines();
                break;
            }
        }

        // Parse optional fields and methods
        let mut fields = Vec::new();
        let mut methods = Vec::new();

        while !matches!(self.peek(), TokenKind::RightBrace | TokenKind::Eof) {
            self.skip_newlines();
            if self.peek() == &TokenKind::Func {
                let method = self.parse_func_decl(false, Visibility::Default)?;
                methods.push(method);
            } else if let TokenKind::Identifier(_) = self.peek() {
                let field = self.parse_field_decl(Visibility::Default)?;
                fields.push(field);
            } else {
                return Err(self.error(format!(
                    "expected field or method in enum body, found {:?}",
                    self.peek()
                )));
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::RightBrace)?;

        Ok(EnumDecl {
            name,
            variants,
            fields,
            methods,
        })
    }

    // --- Import / Export ---

    /// Parses `import { A, B } from "path"` or `import * as alias from "path"`.
    fn parse_import_decl(&mut self) -> Result<Decl, ParseError> {
        let span = self.advance().span; // consume `import`

        if self.peek() == &TokenKind::Star {
            // Wildcard import: `import * as alias from "path"`
            self.advance(); // consume `*`
            self.expect(&TokenKind::As)?;
            let (alias, _) = self.expect_identifier()?;
            self.expect_contextual("from")?;
            let from = self.parse_plain_string()?;
            self.consume_stmt_terminator();
            Ok(Decl {
                kind: DeclKind::WildcardImport(WildcardImportDecl { alias, from }),
                span,
            })
        } else {
            // Named import: `import { A, B } from "path"`
            self.expect(&TokenKind::LeftBrace)?;
            let mut names = Vec::new();
            self.skip_newlines();
            if self.peek() != &TokenKind::RightBrace {
                let (first, _) = self.expect_identifier()?;
                names.push(first);
                while self.peek() == &TokenKind::Comma {
                    self.advance(); // consume `,`
                    self.skip_newlines();
                    let (name, _) = self.expect_identifier()?;
                    names.push(name);
                }
            }
            self.skip_newlines();
            self.expect(&TokenKind::RightBrace)?;
            self.expect_contextual("from")?;
            let from = self.parse_plain_string()?;
            self.consume_stmt_terminator();
            Ok(Decl {
                kind: DeclKind::Import(ImportDecl { names, from }),
                span,
            })
        }
    }

    /// Parses `export <declaration>`.
    fn parse_export_decl(&mut self) -> Result<Decl, ParseError> {
        let span = self.advance().span; // consume `export`
        let inner = self.parse_decl()?;
        Ok(Decl {
            kind: DeclKind::Export(Box::new(inner)),
            span,
        })
    }

    // --- When statement ---

    /// Parses a `when` pattern.
    fn parse_when_pattern(&mut self) -> Result<WhenPattern, ParseError> {
        self.skip_newlines();

        match self.peek().clone() {
            TokenKind::Else => {
                self.advance();
                Ok(WhenPattern::Else)
            }
            TokenKind::Is => {
                self.advance(); // consume `is`
                let (type_name, _) = self.expect_identifier()?;
                let binding = if self.peek() == &TokenKind::LeftParen {
                    self.advance(); // consume `(`
                    let (name, _) = self.expect_identifier()?;
                    self.expect(&TokenKind::RightParen)?;
                    Some(name)
                } else {
                    None
                };
                Ok(WhenPattern::TypeMatch { type_name, binding })
            }
            _ => {
                let first = self.parse_expr()?;

                match self.peek().clone() {
                    // Guard: `x if condition`
                    TokenKind::If => {
                        let binding = match &first.kind {
                            ExprKind::Identifier(name) => name.clone(),
                            _ => {
                                return Err(
                                    self.error("guard pattern requires an identifier binding")
                                );
                            }
                        };
                        self.advance(); // consume `if`
                        let condition = self.parse_expr()?;
                        Ok(WhenPattern::Guard { binding, condition })
                    }
                    // Multiple values: `0, 1, 2`
                    TokenKind::Comma => {
                        let mut values = vec![first];
                        while self.peek() == &TokenKind::Comma {
                            self.advance(); // consume `,`
                            self.skip_newlines();
                            values.push(self.parse_expr()?);
                        }
                        Ok(WhenPattern::MultipleValues(values))
                    }
                    // Check if the expression is a Range -- decompose into WhenPattern::Range
                    _ => match first.kind {
                        ExprKind::Range {
                            start,
                            end,
                            inclusive,
                        } => Ok(WhenPattern::Range {
                            start: *start,
                            end: *end,
                            inclusive,
                        }),
                        _ => Ok(WhenPattern::Value(first)),
                    },
                }
            }
        }
    }
}

/// Maps a token kind to its corresponding binary operator.
fn token_to_binary_op(kind: &TokenKind) -> BinaryOp {
    match kind {
        TokenKind::Plus => BinaryOp::Add,
        TokenKind::Minus => BinaryOp::Subtract,
        TokenKind::Star => BinaryOp::Multiply,
        TokenKind::Slash => BinaryOp::Divide,
        TokenKind::Percent => BinaryOp::Modulo,
        TokenKind::EqualEqual => BinaryOp::Equal,
        TokenKind::BangEqual => BinaryOp::NotEqual,
        TokenKind::Less => BinaryOp::Less,
        TokenKind::Greater => BinaryOp::Greater,
        TokenKind::LessEqual => BinaryOp::LessEqual,
        TokenKind::GreaterEqual => BinaryOp::GreaterEqual,
        TokenKind::AmpAmp => BinaryOp::And,
        TokenKind::PipePipe => BinaryOp::Or,
        _ => unreachable!("not a binary operator: {kind:?}"),
    }
}
