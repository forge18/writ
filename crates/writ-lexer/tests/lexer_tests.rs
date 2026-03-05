use writ_lexer::{LexError, Lexer, TokenKind};

/// Helper: tokenize source and return just the TokenKinds.
fn token_kinds(source: &str) -> Vec<TokenKind> {
    let mut lexer = Lexer::new(source);
    lexer
        .tokenize()
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect()
}

/// Helper: tokenize source and expect a LexError.
fn expect_error(source: &str) -> LexError {
    let mut lexer = Lexer::new(source);
    lexer.tokenize().unwrap_err()
}

#[test]
fn test_keywords() {
    let keywords = vec![
        ("class", TokenKind::Class),
        ("trait", TokenKind::Trait),
        ("enum", TokenKind::Enum),
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
        ("start", TokenKind::Start),
        ("yield", TokenKind::Yield),
        ("true", TokenKind::True),
        ("false", TokenKind::False),
    ];
    for (source, expected) in keywords {
        let kinds = token_kinds(source);
        assert_eq!(
            kinds,
            vec![expected.clone(), TokenKind::Eof],
            "keyword: {source}"
        );
    }

    // `from` is NOT a keyword — it should lex as an identifier.
    let kinds = token_kinds("from");
    assert_eq!(
        kinds,
        vec![TokenKind::Identifier("from".to_owned()), TokenKind::Eof]
    );
}

#[test]
fn test_operators() {
    let operators = vec![
        ("+", TokenKind::Plus),
        ("-", TokenKind::Minus),
        ("*", TokenKind::Star),
        ("/", TokenKind::Slash),
        ("%", TokenKind::Percent),
        ("=", TokenKind::Assign),
        ("==", TokenKind::EqualEqual),
        ("!=", TokenKind::BangEqual),
        ("<", TokenKind::Less),
        (">", TokenKind::Greater),
        ("<=", TokenKind::LessEqual),
        (">=", TokenKind::GreaterEqual),
        ("&&", TokenKind::AmpAmp),
        ("||", TokenKind::PipePipe),
        ("!", TokenKind::Bang),
        ("?", TokenKind::Question),
        ("??", TokenKind::QuestionQuestion),
        ("?.", TokenKind::QuestionDot),
        ("..", TokenKind::DotDot),
        ("...", TokenKind::DotDotDot),
        ("..=", TokenKind::DotDotEqual),
        ("->", TokenKind::Arrow),
        ("=>", TokenKind::FatArrow),
        ("+=", TokenKind::PlusAssign),
        ("-=", TokenKind::MinusAssign),
        ("*=", TokenKind::StarAssign),
        ("/=", TokenKind::SlashAssign),
        ("%=", TokenKind::PercentAssign),
        ("::", TokenKind::ColonColon),
        (".", TokenKind::Dot),
    ];
    for (source, expected) in operators {
        let kinds = token_kinds(source);
        assert_eq!(
            kinds,
            vec![expected.clone(), TokenKind::Eof],
            "operator: {source}"
        );
    }
}

#[test]
fn test_integer_literals() {
    assert_eq!(
        token_kinds("0"),
        vec![TokenKind::IntLiteral(0), TokenKind::Eof]
    );
    assert_eq!(
        token_kinds("100"),
        vec![TokenKind::IntLiteral(100), TokenKind::Eof]
    );
    assert_eq!(
        token_kinds("42"),
        vec![TokenKind::IntLiteral(42), TokenKind::Eof]
    );

    // Negative number: lexed as Minus + IntLiteral (parser handles unary minus).
    assert_eq!(
        token_kinds("-5"),
        vec![TokenKind::Minus, TokenKind::IntLiteral(5), TokenKind::Eof,]
    );

    // Large value.
    assert_eq!(
        token_kinds("2147483647"),
        vec![TokenKind::IntLiteral(2_147_483_647), TokenKind::Eof]
    );
}

#[test]
fn test_float_literals() {
    assert_eq!(
        token_kinds("0.0"),
        vec![TokenKind::FloatLiteral(0.0), TokenKind::Eof]
    );
    assert_eq!(
        token_kinds("3.14"),
        vec![TokenKind::FloatLiteral(3.14), TokenKind::Eof]
    );

    // Negative float: lexed as Minus + FloatLiteral.
    assert_eq!(
        token_kinds("-1.5"),
        vec![
            TokenKind::Minus,
            TokenKind::FloatLiteral(1.5),
            TokenKind::Eof,
        ]
    );

    // Edge case: `0..10` must NOT be a float — it's IntLiteral(0), DotDot, IntLiteral(10).
    assert_eq!(
        token_kinds("0..10"),
        vec![
            TokenKind::IntLiteral(0),
            TokenKind::DotDot,
            TokenKind::IntLiteral(10),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_string_literal() {
    // Plain string.
    assert_eq!(
        token_kinds(r#""hello""#),
        vec![
            TokenKind::StringStart,
            TokenKind::StringLiteral("hello".to_owned()),
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    // String with escape sequences.
    assert_eq!(
        token_kinds(r#""a\nb\tc""#),
        vec![
            TokenKind::StringStart,
            TokenKind::StringLiteral("a\nb\tc".to_owned()),
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    // String with escaped dollar sign.
    assert_eq!(
        token_kinds(r#""Hello \$name""#),
        vec![
            TokenKind::StringStart,
            TokenKind::StringLiteral("Hello $name".to_owned()),
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    // Empty string.
    assert_eq!(
        token_kinds(r#""""#),
        vec![TokenKind::StringStart, TokenKind::StringEnd, TokenKind::Eof,]
    );
}

#[test]
fn test_string_interpolation() {
    // Simple: "Hello $name"
    assert_eq!(
        token_kinds(r#""Hello $name""#),
        vec![
            TokenKind::StringStart,
            TokenKind::StringLiteral("Hello ".to_owned()),
            TokenKind::InterpolationStart,
            TokenKind::Identifier("name".to_owned()),
            TokenKind::InterpolationEnd,
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    // Expression: "${a + b}"
    assert_eq!(
        token_kinds(r#""${a + b}""#),
        vec![
            TokenKind::StringStart,
            TokenKind::InterpolationStart,
            TokenKind::Identifier("a".to_owned()),
            TokenKind::Plus,
            TokenKind::Identifier("b".to_owned()),
            TokenKind::InterpolationEnd,
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    // Mixed: "Health: ${player.health}"
    assert_eq!(
        token_kinds(r#""Health: ${player.health}""#),
        vec![
            TokenKind::StringStart,
            TokenKind::StringLiteral("Health: ".to_owned()),
            TokenKind::InterpolationStart,
            TokenKind::Identifier("player".to_owned()),
            TokenKind::Dot,
            TokenKind::Identifier("health".to_owned()),
            TokenKind::InterpolationEnd,
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    // Multiple interpolations: "Hello $name, you are $age years old"
    assert_eq!(
        token_kinds(r#""Hello $name, you are $age years old""#),
        vec![
            TokenKind::StringStart,
            TokenKind::StringLiteral("Hello ".to_owned()),
            TokenKind::InterpolationStart,
            TokenKind::Identifier("name".to_owned()),
            TokenKind::InterpolationEnd,
            TokenKind::StringLiteral(", you are ".to_owned()),
            TokenKind::InterpolationStart,
            TokenKind::Identifier("age".to_owned()),
            TokenKind::InterpolationEnd,
            TokenKind::StringLiteral(" years old".to_owned()),
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );

    // Interpolation at start: "$name is here"
    assert_eq!(
        token_kinds(r#""$name is here""#),
        vec![
            TokenKind::StringStart,
            TokenKind::InterpolationStart,
            TokenKind::Identifier("name".to_owned()),
            TokenKind::InterpolationEnd,
            TokenKind::StringLiteral(" is here".to_owned()),
            TokenKind::StringEnd,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_multiline_string() {
    let source = r#""""
    Hello world.
    """"#;
    let kinds = token_kinds(source);
    assert_eq!(kinds[0], TokenKind::MultilineStringStart);
    // The content between the triple quotes.
    assert!(matches!(&kinds[1], TokenKind::StringLiteral(s) if s.contains("Hello world.")));
    assert_eq!(kinds[2], TokenKind::MultilineStringEnd);
    assert_eq!(kinds[3], TokenKind::Eof);

    // Multiline with interpolation.
    let source = r#""""
    Player $name has $health health.
    """"#;
    let kinds = token_kinds(source);
    assert_eq!(kinds[0], TokenKind::MultilineStringStart);
    // Should contain StringLiteral, InterpolationStart, Identifier, InterpolationEnd, etc.
    assert!(kinds.contains(&TokenKind::InterpolationStart));
    assert!(kinds.contains(&TokenKind::Identifier("name".to_owned())));
    assert!(kinds.contains(&TokenKind::Identifier("health".to_owned())));
    assert!(kinds.iter().any(|k| *k == TokenKind::MultilineStringEnd));
}

#[test]
fn test_comments_skipped() {
    // Line comment.
    assert_eq!(
        token_kinds("x // comment\ny"),
        vec![
            TokenKind::Identifier("x".to_owned()),
            TokenKind::Newline,
            TokenKind::Identifier("y".to_owned()),
            TokenKind::Eof,
        ]
    );

    // Block comment.
    assert_eq!(
        token_kinds("x /* comment */ y"),
        vec![
            TokenKind::Identifier("x".to_owned()),
            TokenKind::Identifier("y".to_owned()),
            TokenKind::Eof,
        ]
    );

    // Block comment spanning multiple lines.
    assert_eq!(
        token_kinds("x /* multi\nline\ncomment */ y"),
        vec![
            TokenKind::Identifier("x".to_owned()),
            TokenKind::Identifier("y".to_owned()),
            TokenKind::Eof,
        ]
    );

    // Only comments — produces just Eof.
    assert_eq!(token_kinds("// just a comment"), vec![TokenKind::Eof]);
}

#[test]
fn test_span_tracking() {
    let source = "let x = 42\nvar y = 10";
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();

    // `let` at line 1, col 1, length 3
    assert_eq!(tokens[0].kind, TokenKind::Let);
    assert_eq!(tokens[0].span.line, 1);
    assert_eq!(tokens[0].span.column, 1);
    assert_eq!(tokens[0].span.length, 3);

    // `x` at line 1, col 5, length 1
    assert_eq!(tokens[1].kind, TokenKind::Identifier("x".to_owned()));
    assert_eq!(tokens[1].span.line, 1);
    assert_eq!(tokens[1].span.column, 5);
    assert_eq!(tokens[1].span.length, 1);

    // `=` at line 1, col 7, length 1
    assert_eq!(tokens[2].kind, TokenKind::Assign);
    assert_eq!(tokens[2].span.line, 1);
    assert_eq!(tokens[2].span.column, 7);
    assert_eq!(tokens[2].span.length, 1);

    // `42` at line 1, col 9, length 2
    assert_eq!(tokens[3].kind, TokenKind::IntLiteral(42));
    assert_eq!(tokens[3].span.line, 1);
    assert_eq!(tokens[3].span.column, 9);
    assert_eq!(tokens[3].span.length, 2);

    // Newline at line 1, col 11
    assert_eq!(tokens[4].kind, TokenKind::Newline);
    assert_eq!(tokens[4].span.line, 1);
    assert_eq!(tokens[4].span.column, 11);

    // `var` at line 2, col 1, length 3
    assert_eq!(tokens[5].kind, TokenKind::Var);
    assert_eq!(tokens[5].span.line, 2);
    assert_eq!(tokens[5].span.column, 1);
    assert_eq!(tokens[5].span.length, 3);

    // `y` at line 2, col 5
    assert_eq!(tokens[6].kind, TokenKind::Identifier("y".to_owned()));
    assert_eq!(tokens[6].span.line, 2);
    assert_eq!(tokens[6].span.column, 5);
}

#[test]
fn test_newline_terminates_statement() {
    let kinds = token_kinds("let x = 5\nlet y = 10");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Let,
            TokenKind::Identifier("x".to_owned()),
            TokenKind::Assign,
            TokenKind::IntLiteral(5),
            TokenKind::Newline,
            TokenKind::Let,
            TokenKind::Identifier("y".to_owned()),
            TokenKind::Assign,
            TokenKind::IntLiteral(10),
            TokenKind::Eof,
        ]
    );

    // Consecutive newlines collapse to one.
    let kinds = token_kinds("x\n\n\ny");
    assert_eq!(
        kinds,
        vec![
            TokenKind::Identifier("x".to_owned()),
            TokenKind::Newline,
            TokenKind::Identifier("y".to_owned()),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_empty_source() {
    assert_eq!(token_kinds(""), vec![TokenKind::Eof]);
    assert_eq!(token_kinds("   "), vec![TokenKind::Eof]);
    assert_eq!(token_kinds("  \t  "), vec![TokenKind::Eof]);
}

#[test]
fn test_unknown_character() {
    let err = expect_error("#");
    assert!(err.message.contains("unexpected character"));

    let err = expect_error("let x = ~5");
    assert!(err.message.contains("unexpected character"));
}
