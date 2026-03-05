use writ_lexer::Lexer;
use writ_parser::{CallArg, Expr, ExprKind, Literal, Parser, Stmt, StmtKind};

// ── Helpers ───────────────────────────────────────────────────────────

fn parse_expr(source: &str) -> Expr {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_expr().expect("parser failed")
}

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

// ── Yield expression tests ───────────────────────────────────────────

#[test]
fn test_parse_bare_yield() {
    let expr = parse_expr("yield");
    assert!(
        matches!(expr.kind, ExprKind::Yield(None)),
        "expected Yield(None), got {:?}",
        expr.kind
    );
}

#[test]
fn test_parse_yield_with_call() {
    let expr = parse_expr("yield waitForSeconds(2.0)");
    match &expr.kind {
        ExprKind::Yield(Some(inner)) => match &inner.kind {
            ExprKind::Call { callee, args } => {
                assert_eq!(
                    callee.kind,
                    ExprKind::Identifier("waitForSeconds".to_string())
                );
                assert_eq!(args.len(), 1);
                match &args[0] {
                    CallArg::Positional(arg) => match &arg.kind {
                        ExprKind::Literal(Literal::Float(f)) => {
                            assert!((f - 2.0).abs() < f64::EPSILON);
                        }
                        other => panic!("expected float literal, got {other:?}"),
                    },
                    other => panic!("expected positional arg, got {other:?}"),
                }
            }
            other => panic!("expected Call, got {other:?}"),
        },
        other => panic!("expected Yield(Some(...)), got {other:?}"),
    }
}

#[test]
fn test_parse_yield_coroutine_call() {
    let expr = parse_expr("yield getInput()");
    match &expr.kind {
        ExprKind::Yield(Some(inner)) => match &inner.kind {
            ExprKind::Call { callee, args } => {
                assert_eq!(callee.kind, ExprKind::Identifier("getInput".to_string()));
                assert!(args.is_empty());
            }
            other => panic!("expected Call, got {other:?}"),
        },
        other => panic!("expected Yield(Some(...)), got {other:?}"),
    }
}

// ── Start statement tests ────────────────────────────────────────────

#[test]
fn test_parse_start_stmt() {
    let stmt = parse_stmt("start openDoor()");
    match &stmt.kind {
        StmtKind::Start(expr) => match &expr.kind {
            ExprKind::Call { callee, args } => {
                assert_eq!(callee.kind, ExprKind::Identifier("openDoor".to_string()));
                assert!(args.is_empty());
            }
            other => panic!("expected Call, got {other:?}"),
        },
        other => panic!("expected Start, got {other:?}"),
    }
}

#[test]
fn test_parse_start_with_args() {
    let stmt = parse_stmt("start patrolPath(waypoints)");
    match &stmt.kind {
        StmtKind::Start(expr) => match &expr.kind {
            ExprKind::Call { callee, args } => {
                assert_eq!(callee.kind, ExprKind::Identifier("patrolPath".to_string()));
                assert_eq!(args.len(), 1);
                match &args[0] {
                    CallArg::Positional(arg) => {
                        assert_eq!(arg.kind, ExprKind::Identifier("waypoints".to_string()));
                    }
                    other => panic!("expected positional arg, got {other:?}"),
                }
            }
            other => panic!("expected Call, got {other:?}"),
        },
        other => panic!("expected Start, got {other:?}"),
    }
}

// ── Combined tests ───────────────────────────────────────────────────

#[test]
fn test_parse_let_yield() {
    let stmt = parse_stmt("let key = yield getInput()");
    match &stmt.kind {
        StmtKind::Let {
            name, initializer, ..
        } => {
            assert_eq!(name, "key");
            match &initializer.kind {
                ExprKind::Yield(Some(inner)) => match &inner.kind {
                    ExprKind::Call { callee, args } => {
                        assert_eq!(callee.kind, ExprKind::Identifier("getInput".to_string()));
                        assert!(args.is_empty());
                    }
                    other => panic!("expected Call, got {other:?}"),
                },
                other => panic!("expected Yield, got {other:?}"),
            }
        }
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn test_parse_yield_as_expr_stmt() {
    let stmts = parse_stmts("yield waitForSeconds(1.0)");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => match &expr.kind {
            ExprKind::Yield(Some(inner)) => match &inner.kind {
                ExprKind::Call { callee, .. } => {
                    assert_eq!(
                        callee.kind,
                        ExprKind::Identifier("waitForSeconds".to_string())
                    );
                }
                other => panic!("expected Call, got {other:?}"),
            },
            other => panic!("expected Yield, got {other:?}"),
        },
        other => panic!("expected ExprStmt, got {other:?}"),
    }
}

#[test]
fn test_parse_bare_yield_as_expr_stmt() {
    let stmts = parse_stmts("yield");
    assert_eq!(stmts.len(), 1);
    match &stmts[0].kind {
        StmtKind::ExprStmt(expr) => {
            assert!(
                matches!(expr.kind, ExprKind::Yield(None)),
                "expected Yield(None), got {:?}",
                expr.kind
            );
        }
        other => panic!("expected ExprStmt, got {other:?}"),
    }
}
