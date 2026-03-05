use writ_lexer::Lexer;
use writ_parser::{
    ArrayElement, BinaryOp, DictElement, Expr, ExprKind, InterpolationSegment, Literal, ParseError,
    Parser, TypeExpr, UnaryOp,
};

// ── Helpers ───────────────────────────────────────────────────────────

fn parse(source: &str) -> Expr {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_expr().expect("parser failed")
}

fn parse_error(source: &str) -> ParseError {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_expr().expect_err("expected ParseError")
}

fn assert_literal_int(expr: &Expr, expected: i64) {
    assert_eq!(expr.kind, ExprKind::Literal(Literal::Int(expected)));
}

fn assert_literal_float(expr: &Expr, expected: f64) {
    match &expr.kind {
        ExprKind::Literal(Literal::Float(f)) => assert!(
            (*f - expected).abs() < f64::EPSILON,
            "expected {expected}, got {f}"
        ),
        other => panic!("expected float literal, got {other:?}"),
    }
}

fn assert_literal_string(expr: &Expr, expected: &str) {
    match &expr.kind {
        ExprKind::Literal(Literal::String(s)) => assert_eq!(s, expected),
        other => panic!("expected string literal \"{expected}\", got {other:?}"),
    }
}

fn assert_identifier(expr: &Expr, expected: &str) {
    assert_eq!(expr.kind, ExprKind::Identifier(expected.to_string()));
}

// ── Literal tests ─────────────────────────────────────────────────────

#[test]
fn test_integer_literal() {
    let expr = parse("42");
    assert_literal_int(&expr, 42);
}

#[test]
fn test_float_literal() {
    let expr = parse("3.14");
    assert_literal_float(&expr, 3.14);
}

#[test]
fn test_bool_literals() {
    let t = parse("true");
    assert_eq!(t.kind, ExprKind::Literal(Literal::Bool(true)));

    let f = parse("false");
    assert_eq!(f.kind, ExprKind::Literal(Literal::Bool(false)));
}

#[test]
fn test_string_literal() {
    let expr = parse(r#""hello""#);
    assert_eq!(
        expr.kind,
        ExprKind::Literal(Literal::String("hello".to_string()))
    );
}

#[test]
fn test_empty_string() {
    let expr = parse(r#""""#);
    assert_eq!(expr.kind, ExprKind::Literal(Literal::String(String::new())));
}

// ── String interpolation tests ────────────────────────────────────────

#[test]
fn test_string_interpolation() {
    let expr = parse(r#""Hello $name""#);
    match &expr.kind {
        ExprKind::StringInterpolation(segments) => {
            assert_eq!(segments.len(), 2);
            assert_eq!(
                segments[0],
                InterpolationSegment::Literal("Hello ".to_string())
            );
            match &segments[1] {
                InterpolationSegment::Expression(e) => {
                    assert_identifier(e, "name");
                }
                other => panic!("expected expression segment, got {other:?}"),
            }
        }
        other => panic!("expected StringInterpolation, got {other:?}"),
    }
}

#[test]
fn test_string_interpolation_expr() {
    let expr = parse(r#""${a + b}""#);
    match &expr.kind {
        ExprKind::StringInterpolation(segments) => {
            // May or may not have an empty literal segment depending on lexer output.
            // Find the expression segment.
            let expr_seg = segments
                .iter()
                .find(|s| matches!(s, InterpolationSegment::Expression(_)))
                .expect("expected expression segment");

            match expr_seg {
                InterpolationSegment::Expression(e) => match &e.kind {
                    ExprKind::Binary { op, lhs, rhs } => {
                        assert_eq!(*op, BinaryOp::Add);
                        assert_identifier(lhs, "a");
                        assert_identifier(rhs, "b");
                    }
                    other => panic!("expected binary add, got {other:?}"),
                },
                _ => unreachable!(),
            }
        }
        other => panic!("expected StringInterpolation, got {other:?}"),
    }
}

// ── Binary operator tests ─────────────────────────────────────────────

#[test]
fn test_binary_add() {
    let expr = parse("1 + 2");
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinaryOp::Add);
            assert_literal_int(lhs, 1);
            assert_literal_int(rhs, 2);
        }
        other => panic!("expected binary add, got {other:?}"),
    }
}

#[test]
fn test_binary_precedence() {
    // 1 + 2 * 3 should parse as 1 + (2 * 3)
    let expr = parse("1 + 2 * 3");
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinaryOp::Add);
            assert_literal_int(lhs, 1);
            match &rhs.kind {
                ExprKind::Binary { op, lhs, rhs } => {
                    assert_eq!(*op, BinaryOp::Multiply);
                    assert_literal_int(lhs, 2);
                    assert_literal_int(rhs, 3);
                }
                other => panic!("expected multiply, got {other:?}"),
            }
        }
        other => panic!("expected add, got {other:?}"),
    }
}

// ── Unary operator tests ──────────────────────────────────────────────

#[test]
fn test_unary_negate() {
    let expr = parse("-5");
    match &expr.kind {
        ExprKind::Unary { op, operand } => {
            assert_eq!(*op, UnaryOp::Negate);
            assert_literal_int(operand, 5);
        }
        other => panic!("expected unary negate, got {other:?}"),
    }
}

#[test]
fn test_unary_not() {
    let expr = parse("!true");
    match &expr.kind {
        ExprKind::Unary { op, operand } => {
            assert_eq!(*op, UnaryOp::Not);
            assert_eq!(operand.kind, ExprKind::Literal(Literal::Bool(true)));
        }
        other => panic!("expected unary not, got {other:?}"),
    }
}

#[test]
fn test_unary_in_binary() {
    // -a + b should parse as (-a) + b
    let expr = parse("-a + b");
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinaryOp::Add);
            match &lhs.kind {
                ExprKind::Unary { op, operand } => {
                    assert_eq!(*op, UnaryOp::Negate);
                    assert_identifier(operand, "a");
                }
                other => panic!("expected unary negate, got {other:?}"),
            }
            assert_identifier(rhs, "b");
        }
        other => panic!("expected binary add, got {other:?}"),
    }
}

// ── Grouped expression tests ─────────────────────────────────────────

#[test]
fn test_grouped() {
    // (1 + 2) * 3
    let expr = parse("(1 + 2) * 3");
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinaryOp::Multiply);
            match &lhs.kind {
                ExprKind::Grouped(inner) => match &inner.kind {
                    ExprKind::Binary { op, lhs, rhs } => {
                        assert_eq!(*op, BinaryOp::Add);
                        assert_literal_int(lhs, 1);
                        assert_literal_int(rhs, 2);
                    }
                    other => panic!("expected add inside group, got {other:?}"),
                },
                other => panic!("expected grouped, got {other:?}"),
            }
            assert_literal_int(rhs, 3);
        }
        other => panic!("expected multiply, got {other:?}"),
    }
}

// ── Ternary tests ─────────────────────────────────────────────────────

#[test]
fn test_ternary() {
    let expr = parse("x ? 1 : 2");
    match &expr.kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            assert_identifier(condition, "x");
            assert_literal_int(then_expr, 1);
            assert_literal_int(else_expr, 2);
        }
        other => panic!("expected ternary, got {other:?}"),
    }
}

#[test]
fn test_nested_ternary() {
    // a ? b ? c : d : e should parse as a ? (b ? c : d) : e
    let expr = parse("a ? b ? c : d : e");
    match &expr.kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            assert_identifier(condition, "a");
            // then_expr should be another ternary: b ? c : d
            match &then_expr.kind {
                ExprKind::Ternary {
                    condition,
                    then_expr,
                    else_expr,
                } => {
                    assert_identifier(condition, "b");
                    assert_identifier(then_expr, "c");
                    assert_identifier(else_expr, "d");
                }
                other => panic!("expected inner ternary, got {other:?}"),
            }
            assert_identifier(else_expr, "e");
        }
        other => panic!("expected ternary, got {other:?}"),
    }
}

// ── Range tests ───────────────────────────────────────────────────────

#[test]
fn test_range_exclusive() {
    let expr = parse("0..10");
    match &expr.kind {
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            assert!(!inclusive);
            assert_literal_int(start, 0);
            assert_literal_int(end, 10);
        }
        other => panic!("expected range, got {other:?}"),
    }
}

#[test]
fn test_range_inclusive() {
    let expr = parse("0..=10");
    match &expr.kind {
        ExprKind::Range {
            start,
            end,
            inclusive,
        } => {
            assert!(inclusive);
            assert_literal_int(start, 0);
            assert_literal_int(end, 10);
        }
        other => panic!("expected range, got {other:?}"),
    }
}

// ── Null coalesce tests ──────────────────────────────────────────────

#[test]
fn test_null_coalesce() {
    let expr = parse("a ?? b");
    match &expr.kind {
        ExprKind::NullCoalesce { lhs, rhs } => {
            assert_identifier(lhs, "a");
            assert_identifier(rhs, "b");
        }
        other => panic!("expected null coalesce, got {other:?}"),
    }
}

#[test]
fn test_null_coalesce_chain() {
    // a ?? b ?? c should parse as (a ?? b) ?? c (left-associative)
    let expr = parse("a ?? b ?? c");
    match &expr.kind {
        ExprKind::NullCoalesce { lhs, rhs } => {
            // Outer rhs should be c
            assert_identifier(rhs, "c");
            // Outer lhs should be (a ?? b)
            match &lhs.kind {
                ExprKind::NullCoalesce { lhs, rhs } => {
                    assert_identifier(lhs, "a");
                    assert_identifier(rhs, "b");
                }
                other => panic!("expected inner null coalesce, got {other:?}"),
            }
        }
        other => panic!("expected null coalesce, got {other:?}"),
    }
}

// ── Access tests ──────────────────────────────────────────────────────

#[test]
fn test_safe_access() {
    let expr = parse("a?.b");
    match &expr.kind {
        ExprKind::SafeAccess { object, member } => {
            assert_identifier(object, "a");
            assert_eq!(member, "b");
        }
        other => panic!("expected safe access, got {other:?}"),
    }
}

#[test]
fn test_chained_access() {
    // a?.b?.c
    let expr = parse("a?.b?.c");
    match &expr.kind {
        ExprKind::SafeAccess { object, member } => {
            assert_eq!(member, "c");
            match &object.kind {
                ExprKind::SafeAccess { object, member } => {
                    assert_identifier(object, "a");
                    assert_eq!(member, "b");
                }
                other => panic!("expected inner safe access, got {other:?}"),
            }
        }
        other => panic!("expected safe access, got {other:?}"),
    }
}

#[test]
fn test_member_access() {
    let expr = parse("a.b");
    match &expr.kind {
        ExprKind::MemberAccess { object, member } => {
            assert_identifier(object, "a");
            assert_eq!(member, "b");
        }
        other => panic!("expected member access, got {other:?}"),
    }
}

// ── Cast test ─────────────────────────────────────────────────────────

#[test]
fn test_cast() {
    let expr = parse("x as float");
    match &expr.kind {
        ExprKind::Cast { expr, target_type } => {
            assert_identifier(expr, "x");
            assert_eq!(*target_type, TypeExpr::Simple("float".to_string()));
        }
        other => panic!("expected cast, got {other:?}"),
    }
}

// ── Complex precedence test ───────────────────────────────────────────

#[test]
fn test_complex_precedence() {
    // a || b && c == d + 1
    // should parse as: a || (b && (c == (d + 1)))
    let expr = parse("a || b && c == d + 1");
    match &expr.kind {
        ExprKind::Binary { op, lhs, rhs } => {
            assert_eq!(*op, BinaryOp::Or);
            assert_identifier(lhs, "a");
            // rhs: b && (c == (d + 1))
            match &rhs.kind {
                ExprKind::Binary { op, lhs, rhs } => {
                    assert_eq!(*op, BinaryOp::And);
                    assert_identifier(lhs, "b");
                    // rhs: c == (d + 1)
                    match &rhs.kind {
                        ExprKind::Binary { op, lhs, rhs } => {
                            assert_eq!(*op, BinaryOp::Equal);
                            assert_identifier(lhs, "c");
                            // rhs: d + 1
                            match &rhs.kind {
                                ExprKind::Binary { op, lhs, rhs } => {
                                    assert_eq!(*op, BinaryOp::Add);
                                    assert_identifier(lhs, "d");
                                    assert_literal_int(rhs, 1);
                                }
                                other => panic!("expected add, got {other:?}"),
                            }
                        }
                        other => panic!("expected equal, got {other:?}"),
                    }
                }
                other => panic!("expected and, got {other:?}"),
            }
        }
        other => panic!("expected or, got {other:?}"),
    }
}

// ── Array literal tests ──────────────────────────────────────────────

#[test]
fn test_array_literal_empty() {
    let expr = parse("[]");
    match &expr.kind {
        ExprKind::ArrayLiteral(elements) => {
            assert!(elements.is_empty());
        }
        other => panic!("expected array literal, got {other:?}"),
    }
}

#[test]
fn test_array_literal_elements() {
    let expr = parse("[1, 2, 3]");
    match &expr.kind {
        ExprKind::ArrayLiteral(elements) => {
            assert_eq!(elements.len(), 3);
            match &elements[0] {
                ArrayElement::Expr(e) => assert_literal_int(e, 1),
                other => panic!("expected expr, got {other:?}"),
            }
            match &elements[1] {
                ArrayElement::Expr(e) => assert_literal_int(e, 2),
                other => panic!("expected expr, got {other:?}"),
            }
            match &elements[2] {
                ArrayElement::Expr(e) => assert_literal_int(e, 3),
                other => panic!("expected expr, got {other:?}"),
            }
        }
        other => panic!("expected array literal, got {other:?}"),
    }
}

#[test]
fn test_array_literal_spread() {
    let expr = parse("[...a, 4]");
    match &expr.kind {
        ExprKind::ArrayLiteral(elements) => {
            assert_eq!(elements.len(), 2);
            match &elements[0] {
                ArrayElement::Spread(e) => assert_identifier(e, "a"),
                other => panic!("expected spread, got {other:?}"),
            }
            match &elements[1] {
                ArrayElement::Expr(e) => assert_literal_int(e, 4),
                other => panic!("expected expr, got {other:?}"),
            }
        }
        other => panic!("expected array literal, got {other:?}"),
    }
}

// ── Dict literal tests ──────────────────────────────────────────────

#[test]
fn test_dict_literal_empty() {
    let expr = parse("{}");
    match &expr.kind {
        ExprKind::DictLiteral(entries) => {
            assert!(entries.is_empty());
        }
        other => panic!("expected dict literal, got {other:?}"),
    }
}

#[test]
fn test_dict_literal_entries() {
    let expr = parse(r#"{"a": 1, "b": 2}"#);
    match &expr.kind {
        ExprKind::DictLiteral(entries) => {
            assert_eq!(entries.len(), 2);
            match &entries[0] {
                DictElement::KeyValue { key, value } => {
                    assert_literal_string(key, "a");
                    assert_literal_int(value, 1);
                }
                other => panic!("expected key-value, got {other:?}"),
            }
            match &entries[1] {
                DictElement::KeyValue { key, value } => {
                    assert_literal_string(key, "b");
                    assert_literal_int(value, 2);
                }
                other => panic!("expected key-value, got {other:?}"),
            }
        }
        other => panic!("expected dict literal, got {other:?}"),
    }
}

#[test]
fn test_dict_literal_spread() {
    let expr = parse(r#"{...d1, "c": 3}"#);
    match &expr.kind {
        ExprKind::DictLiteral(entries) => {
            assert_eq!(entries.len(), 2);
            match &entries[0] {
                DictElement::Spread(e) => assert_identifier(e, "d1"),
                other => panic!("expected spread, got {other:?}"),
            }
            match &entries[1] {
                DictElement::KeyValue { key, value } => {
                    assert_literal_string(key, "c");
                    assert_literal_int(value, 3);
                }
                other => panic!("expected key-value, got {other:?}"),
            }
        }
        other => panic!("expected dict literal, got {other:?}"),
    }
}

// ── Namespace access tests ──────────────────────────────────────────

#[test]
fn test_namespace_access() {
    let expr = parse("enemy::Enemy");
    match &expr.kind {
        ExprKind::NamespaceAccess { namespace, member } => {
            assert_eq!(namespace, "enemy");
            assert_eq!(member, "Enemy");
        }
        other => panic!("expected namespace access, got {other:?}"),
    }
}

// ── Error tests ───────────────────────────────────────────────────────

#[test]
fn test_parse_error_missing_rhs() {
    let err = parse_error("1 +");
    assert!(
        err.message.contains("expected expression"),
        "error message was: {}",
        err.message
    );
}
