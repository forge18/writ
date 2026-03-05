use writ_lexer::Lexer;
use writ_parser::{
    AssignOp, ElseBranch, ExprKind, Literal, Parser, Stmt, StmtKind, TypeExpr, WhenBody,
    WhenPattern,
};

// ── Helpers ───────────────────────────────────────────────────────────

fn parse_stmt(source: &str) -> Stmt {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_stmt().expect("parser failed")
}

fn parse_stmts(source: &str) -> Vec<Stmt> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_program().expect("parser failed")
}

// ── Variable declaration tests ────────────────────────────────────────

#[test]
fn test_let_decl_with_type() {
    let stmt = parse_stmt(r#"let name: string = "Hero""#);
    match &stmt.kind {
        StmtKind::Let {
            name,
            type_annotation,
            initializer,
            ..
        } => {
            assert_eq!(name, "name");
            assert_eq!(
                *type_annotation,
                Some(TypeExpr::Simple("string".to_string()))
            );
            assert_eq!(
                initializer.kind,
                ExprKind::Literal(Literal::String("Hero".to_string()))
            );
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn test_let_decl_inferred() {
    let stmt = parse_stmt(r#"let name = "Hero""#);
    match &stmt.kind {
        StmtKind::Let {
            name,
            type_annotation,
            initializer,
            ..
        } => {
            assert_eq!(name, "name");
            assert!(type_annotation.is_none());
            assert_eq!(
                initializer.kind,
                ExprKind::Literal(Literal::String("Hero".to_string()))
            );
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn test_var_decl() {
    let stmt = parse_stmt("var health: float = 100.0");
    match &stmt.kind {
        StmtKind::Var {
            name,
            type_annotation,
            initializer,
            ..
        } => {
            assert_eq!(name, "health");
            assert_eq!(
                *type_annotation,
                Some(TypeExpr::Simple("float".to_string()))
            );
            match &initializer.kind {
                ExprKind::Literal(Literal::Float(f)) => {
                    assert!((f - 100.0).abs() < f64::EPSILON);
                }
                other => panic!("expected float literal, got {other:?}"),
            }
        }
        other => panic!("expected Var, got {other:?}"),
    }
}

#[test]
fn test_const_decl() {
    let stmt = parse_stmt("const MAX_HEALTH = 100.0");
    match &stmt.kind {
        StmtKind::Const { name, initializer } => {
            assert_eq!(name, "MAX_HEALTH");
            match &initializer.kind {
                ExprKind::Literal(Literal::Float(f)) => {
                    assert!((f - 100.0).abs() < f64::EPSILON);
                }
                other => panic!("expected float literal, got {other:?}"),
            }
        }
        other => panic!("expected Const, got {other:?}"),
    }
}

// ── Assignment tests ──────────────────────────────────────────────────

#[test]
fn test_assignment() {
    let stmt = parse_stmt("x = 5");
    match &stmt.kind {
        StmtKind::Assignment { target, op, value } => {
            assert_eq!(target.kind, ExprKind::Identifier("x".to_string()));
            assert_eq!(*op, AssignOp::Assign);
            assert_eq!(value.kind, ExprKind::Literal(Literal::Int(5)));
        }
        other => panic!("expected Assignment, got {other:?}"),
    }
}

#[test]
fn test_compound_assignment() {
    let cases = [
        ("health += 10", AssignOp::AddAssign),
        ("health -= amount", AssignOp::SubAssign),
        ("score *= 2", AssignOp::MulAssign),
        ("total /= count", AssignOp::DivAssign),
        ("value %= 3", AssignOp::ModAssign),
    ];

    for (source, expected_op) in cases {
        let stmt = parse_stmt(source);
        match &stmt.kind {
            StmtKind::Assignment { op, .. } => {
                assert_eq!(*op, expected_op, "failed for: {source}");
            }
            other => panic!("expected Assignment for `{source}`, got {other:?}"),
        }
    }
}

// ── If/else tests ─────────────────────────────────────────────────────

#[test]
fn test_if_else() {
    let stmt = parse_stmt("if health <= 0 { die() } else { heal() }");
    match &stmt.kind {
        StmtKind::If {
            condition,
            then_block,
            else_branch,
        } => {
            // Condition is a binary comparison
            assert!(matches!(&condition.kind, ExprKind::Binary { .. }));
            assert_eq!(then_block.len(), 1);
            assert!(matches!(&else_branch, Some(ElseBranch::ElseBlock(stmts)) if stmts.len() == 1));
        }
        other => panic!("expected If, got {other:?}"),
    }
}

#[test]
fn test_if_else_if_else() {
    let source = r#"if health <= 0 {
        die()
    } else if health < 25 {
        warn()
    } else {
        heal()
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::If {
            then_block,
            else_branch,
            ..
        } => {
            assert_eq!(then_block.len(), 1);
            match else_branch {
                Some(ElseBranch::ElseIf(inner)) => match &inner.kind {
                    StmtKind::If {
                        then_block,
                        else_branch,
                        ..
                    } => {
                        assert_eq!(then_block.len(), 1);
                        assert!(
                            matches!(else_branch, Some(ElseBranch::ElseBlock(stmts)) if stmts.len() == 1)
                        );
                    }
                    other => panic!("expected inner If, got {other:?}"),
                },
                other => panic!("expected ElseIf, got {other:?}"),
            }
        }
        other => panic!("expected If, got {other:?}"),
    }
}

// ── Loop tests ────────────────────────────────────────────────────────

#[test]
fn test_while() {
    let stmt = parse_stmt("while health > 0 { update() }");
    match &stmt.kind {
        StmtKind::While { condition, body } => {
            assert!(matches!(&condition.kind, ExprKind::Binary { .. }));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected While, got {other:?}"),
    }
}

#[test]
fn test_for_in_array() {
    let stmt = parse_stmt("for item in items { print(item) }");
    match &stmt.kind {
        StmtKind::For {
            variable,
            iterable,
            body,
        } => {
            assert_eq!(variable, "item");
            assert_eq!(iterable.kind, ExprKind::Identifier("items".to_string()));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected For, got {other:?}"),
    }
}

#[test]
fn test_for_in_range() {
    let stmt = parse_stmt("for i in 0..10 { print(i) }");
    match &stmt.kind {
        StmtKind::For {
            variable,
            iterable,
            body,
        } => {
            assert_eq!(variable, "i");
            assert!(matches!(
                &iterable.kind,
                ExprKind::Range {
                    inclusive: false,
                    ..
                }
            ));
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected For, got {other:?}"),
    }
}

// ── When tests ────────────────────────────────────────────────────────

#[test]
fn test_when_value_matching() {
    let source = r#"when health {
        0 => print("Dead")
        100 => print("Full")
        else => print("OK")
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::When { subject, arms } => {
            assert!(subject.is_some());
            assert_eq!(arms.len(), 3);
            assert!(matches!(&arms[0].pattern, WhenPattern::Value(_)));
            assert!(matches!(&arms[1].pattern, WhenPattern::Value(_)));
            assert!(matches!(&arms[2].pattern, WhenPattern::Else));
        }
        other => panic!("expected When, got {other:?}"),
    }
}

#[test]
fn test_when_multiple_values() {
    let source = r#"when health {
        0, 1, 2 => print("Critical")
        else => print("OK")
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::When { arms, .. } => {
            assert_eq!(arms.len(), 2);
            match &arms[0].pattern {
                WhenPattern::MultipleValues(values) => assert_eq!(values.len(), 3),
                other => panic!("expected MultipleValues, got {other:?}"),
            }
        }
        other => panic!("expected When, got {other:?}"),
    }
}

#[test]
fn test_when_range() {
    let source = r#"when health {
        0..25 => print("Low")
        else => print("OK")
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::When { arms, .. } => {
            assert_eq!(arms.len(), 2);
            match &arms[0].pattern {
                WhenPattern::Range {
                    start,
                    end,
                    inclusive,
                } => {
                    assert!(!inclusive);
                    assert_eq!(start.kind, ExprKind::Literal(Literal::Int(0)));
                    assert_eq!(end.kind, ExprKind::Literal(Literal::Int(25)));
                }
                other => panic!("expected Range, got {other:?}"),
            }
        }
        other => panic!("expected When, got {other:?}"),
    }
}

#[test]
fn test_when_type_match() {
    let source = r#"when result {
        is Success(value) => print(value)
        is Error(msg) => print(msg)
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::When { arms, .. } => {
            assert_eq!(arms.len(), 2);
            match &arms[0].pattern {
                WhenPattern::TypeMatch { type_name, binding } => {
                    assert_eq!(type_name, "Success");
                    assert_eq!(binding.as_deref(), Some("value"));
                }
                other => panic!("expected TypeMatch, got {other:?}"),
            }
            match &arms[1].pattern {
                WhenPattern::TypeMatch { type_name, binding } => {
                    assert_eq!(type_name, "Error");
                    assert_eq!(binding.as_deref(), Some("msg"));
                }
                other => panic!("expected TypeMatch, got {other:?}"),
            }
        }
        other => panic!("expected When, got {other:?}"),
    }
}

#[test]
fn test_when_guard() {
    let source = r#"when health {
        x if x < 0 => print("Invalid")
        else => print("OK")
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::When { arms, .. } => {
            assert_eq!(arms.len(), 2);
            match &arms[0].pattern {
                WhenPattern::Guard { binding, condition } => {
                    assert_eq!(binding, "x");
                    assert!(matches!(&condition.kind, ExprKind::Binary { .. }));
                }
                other => panic!("expected Guard, got {other:?}"),
            }
        }
        other => panic!("expected When, got {other:?}"),
    }
}

#[test]
fn test_when_no_subject() {
    let source = r#"when {
        health == 100 => print("Full")
        else => print("Damaged")
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::When { subject, arms } => {
            assert!(subject.is_none());
            assert_eq!(arms.len(), 2);
        }
        other => panic!("expected When, got {other:?}"),
    }
}

#[test]
fn test_when_multiline_arm() {
    let source = r#"when result {
        is Success(value) => {
            print(value)
            log(value)
        }
        is Error(msg) => print(msg)
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::When { arms, .. } => {
            assert_eq!(arms.len(), 2);
            match &arms[0].body {
                WhenBody::Block(stmts) => assert_eq!(stmts.len(), 2),
                other => panic!("expected Block body, got {other:?}"),
            }
            assert!(matches!(&arms[1].body, WhenBody::Expr(_)));
        }
        other => panic!("expected When, got {other:?}"),
    }
}

// ── Return, break, continue tests ─────────────────────────────────────

#[test]
fn test_return_value() {
    let stmt = parse_stmt("return 42");
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            assert_eq!(expr.kind, ExprKind::Literal(Literal::Int(42)));
        }
        other => panic!("expected Return with value, got {other:?}"),
    }
}

#[test]
fn test_return_void() {
    let stmt = parse_stmt("return");
    match &stmt.kind {
        StmtKind::Return(None) => {}
        other => panic!("expected Return(None), got {other:?}"),
    }
}

#[test]
fn test_break() {
    let stmt = parse_stmt("break");
    assert!(matches!(&stmt.kind, StmtKind::Break));
}

#[test]
fn test_continue() {
    let stmt = parse_stmt("continue");
    assert!(matches!(&stmt.kind, StmtKind::Continue));
}

// ── Optional semicolons test ──────────────────────────────────────────

#[test]
fn test_optional_semicolons() {
    let without_semicolons = parse_stmts("let x = 1\nlet y = 2\nx = 3");
    let with_semicolons = parse_stmts("let x = 1; let y = 2; x = 3;");

    assert_eq!(without_semicolons.len(), 3);
    assert_eq!(with_semicolons.len(), 3);

    // Verify structural equivalence (ignoring spans)
    for (a, b) in without_semicolons.iter().zip(with_semicolons.iter()) {
        assert_eq!(
            std::mem::discriminant(&a.kind),
            std::mem::discriminant(&b.kind),
        );
    }

    // Verify the specific kinds match
    assert!(matches!(&without_semicolons[0].kind, StmtKind::Let { name, .. } if name == "x"));
    assert!(matches!(&with_semicolons[0].kind, StmtKind::Let { name, .. } if name == "x"));
    assert!(matches!(&without_semicolons[1].kind, StmtKind::Let { name, .. } if name == "y"));
    assert!(matches!(&with_semicolons[1].kind, StmtKind::Let { name, .. } if name == "y"));
    assert!(matches!(
        &without_semicolons[2].kind,
        StmtKind::Assignment { .. }
    ));
    assert!(matches!(
        &with_semicolons[2].kind,
        StmtKind::Assignment { .. }
    ));
}
