use writ_lexer::Lexer;
use writ_parser::{
    CallArg, Decl, DeclKind, ExprKind, LambdaBody, Literal, Parser, Stmt, StmtKind, TypeExpr,
    Visibility,
};

// ── Helpers ───────────────────────────────────────────────────────────

fn parse_stmt(source: &str) -> Stmt {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_stmt().expect("parser failed")
}

fn parse_decl(source: &str) -> Decl {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_decl().expect("parser failed")
}

fn parse_file(source: &str) -> Vec<Decl> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_file().expect("parser failed")
}

fn parse_expr(source: &str) -> writ_parser::Expr {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    parser.parse_expr().expect("parser failed")
}

// ── Function declaration tests ────────────────────────────────────────

#[test]
fn test_func_decl() {
    let stmt = parse_stmt("func takeDamage(amount: float) { health -= amount }");
    match &stmt.kind {
        StmtKind::Func(func) => {
            assert_eq!(func.name, "takeDamage");
            assert_eq!(func.params.len(), 1);
            assert_eq!(func.params[0].name, "amount");
            assert_eq!(
                func.params[0].type_annotation,
                TypeExpr::Simple("float".to_string())
            );
            assert!(!func.params[0].is_variadic);
            assert!(func.return_type.is_none());
            assert_eq!(func.body.len(), 1);
            assert!(!func.is_static);
        }
        other => panic!("expected Func, got {other:?}"),
    }
}

#[test]
fn test_func_with_return_type() {
    let source = r#"func divide(a: float, b: float) -> Result<float> {
        return a
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Func(func) => {
            assert_eq!(func.name, "divide");
            assert_eq!(func.params.len(), 2);
            assert_eq!(func.params[0].name, "a");
            assert_eq!(func.params[1].name, "b");
            match &func.return_type {
                Some(TypeExpr::Generic { name, args }) => {
                    assert_eq!(name, "Result");
                    assert_eq!(args.len(), 1);
                    assert_eq!(args[0], TypeExpr::Simple("float".to_string()));
                }
                other => panic!("expected Generic return type, got {other:?}"),
            }
        }
        other => panic!("expected Func, got {other:?}"),
    }
}

#[test]
fn test_func_variadic() {
    let source = r#"func sum(...numbers: int) -> int {
        return 0
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Func(func) => {
            assert_eq!(func.name, "sum");
            assert_eq!(func.params.len(), 1);
            assert!(func.params[0].is_variadic);
            assert_eq!(func.params[0].name, "numbers");
            assert_eq!(
                func.params[0].type_annotation,
                TypeExpr::Simple("int".to_string())
            );
            assert_eq!(func.return_type, Some(TypeExpr::Simple("int".to_string())));
        }
        other => panic!("expected Func, got {other:?}"),
    }
}

#[test]
fn test_static_func() {
    let source = r#"static func create(name: string) -> Player {
        return name
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Func(func) => {
            assert!(func.is_static);
            assert_eq!(func.name, "create");
            assert_eq!(func.params.len(), 1);
            assert_eq!(
                func.return_type,
                Some(TypeExpr::Simple("Player".to_string()))
            );
        }
        other => panic!("expected Func, got {other:?}"),
    }
}

// ── Lambda expression tests ───────────────────────────────────────────

#[test]
fn test_lambda_single_expr() {
    let source = "let double = (x: float) => x * 2";
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Let { initializer, .. } => match &initializer.kind {
            ExprKind::Lambda { params, body } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "x");
                assert_eq!(
                    params[0].type_annotation,
                    TypeExpr::Simple("float".to_string())
                );
                assert!(matches!(body, LambdaBody::Expr(_)));
            }
            other => panic!("expected Lambda, got {other:?}"),
        },
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn test_lambda_block() {
    let source = r#"let onDamage = (amount: float) => {
        print(amount)
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Let { initializer, .. } => match &initializer.kind {
            ExprKind::Lambda { params, body } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "amount");
                match body {
                    LambdaBody::Block(stmts) => assert_eq!(stmts.len(), 1),
                    other => panic!("expected Block body, got {other:?}"),
                }
            }
            other => panic!("expected Lambda, got {other:?}"),
        },
        other => panic!("expected Let, got {other:?}"),
    }
}

// ── Tuple tests ───────────────────────────────────────────────────────

#[test]
fn test_tuple_decl() {
    let stmt = parse_stmt("let point = (10.0, 20.0)");
    match &stmt.kind {
        StmtKind::Let { initializer, .. } => match &initializer.kind {
            ExprKind::Tuple(elements) => {
                assert_eq!(elements.len(), 2);
                match &elements[0].kind {
                    ExprKind::Literal(Literal::Float(f)) => {
                        assert!((f - 10.0).abs() < f64::EPSILON)
                    }
                    other => panic!("expected float, got {other:?}"),
                }
                match &elements[1].kind {
                    ExprKind::Literal(Literal::Float(f)) => {
                        assert!((f - 20.0).abs() < f64::EPSILON)
                    }
                    other => panic!("expected float, got {other:?}"),
                }
            }
            other => panic!("expected Tuple, got {other:?}"),
        },
        other => panic!("expected Let, got {other:?}"),
    }
}

#[test]
fn test_tuple_destructure() {
    let stmt = parse_stmt("let (x, y) = point");
    match &stmt.kind {
        StmtKind::LetDestructure { names, initializer } => {
            assert_eq!(names, &["x", "y"]);
            assert_eq!(initializer.kind, ExprKind::Identifier("point".to_string()));
        }
        other => panic!("expected LetDestructure, got {other:?}"),
    }
}

// ── Class declaration tests ──────────────────────────────────────────

#[test]
fn test_class_empty() {
    let stmt = parse_stmt("class Foo {}");
    match &stmt.kind {
        StmtKind::Class(class) => {
            assert_eq!(class.name, "Foo");
            assert!(class.extends.is_none());
            assert!(class.traits.is_empty());
            assert!(class.fields.is_empty());
            assert!(class.methods.is_empty());
        }
        other => panic!("expected Class, got {other:?}"),
    }
}

#[test]
fn test_class_extends() {
    let source = r#"class Player extends Entity {
        public name: string
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Class(class) => {
            assert_eq!(class.name, "Player");
            assert_eq!(class.extends.as_deref(), Some("Entity"));
            assert_eq!(class.fields.len(), 1);
        }
        other => panic!("expected Class, got {other:?}"),
    }
}

#[test]
fn test_class_with_traits() {
    let source = r#"class Player extends Entity with Damageable, Updatable {
        public name: string
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Class(class) => {
            assert_eq!(class.name, "Player");
            assert_eq!(class.extends.as_deref(), Some("Entity"));
            assert_eq!(class.traits, vec!["Damageable", "Updatable"]);
        }
        other => panic!("expected Class, got {other:?}"),
    }
}

#[test]
fn test_class_field_visibility() {
    let source = r#"class Player {
        public name: string
        private speed: float = 5.0
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Class(class) => {
            assert_eq!(class.fields.len(), 2);
            assert_eq!(class.fields[0].name, "name");
            assert_eq!(class.fields[0].visibility, Visibility::Public);
            assert!(class.fields[0].default.is_none());
            assert_eq!(class.fields[1].name, "speed");
            assert_eq!(class.fields[1].visibility, Visibility::Private);
            assert!(class.fields[1].default.is_some());
        }
        other => panic!("expected Class, got {other:?}"),
    }
}

#[test]
fn test_class_field_setter() {
    let source = r#"class Player {
        public health: float = 100.0
            set(value) { field = value }
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Class(class) => {
            assert_eq!(class.fields.len(), 1);
            let field = &class.fields[0];
            assert_eq!(field.name, "health");
            assert!(field.setter.is_some());
            let setter = field.setter.as_ref().unwrap();
            assert_eq!(setter.param_name, "value");
            assert_eq!(setter.body.len(), 1);
        }
        other => panic!("expected Class, got {other:?}"),
    }
}

#[test]
fn test_class_constructor_call() {
    let expr = parse_expr(r#"Player(name: "Hero")"#);
    match &expr.kind {
        ExprKind::Call { callee, args } => {
            assert_eq!(callee.kind, ExprKind::Identifier("Player".to_string()));
            assert_eq!(args.len(), 1);
            match &args[0] {
                CallArg::Named { name, value } => {
                    assert_eq!(name, "name");
                    assert_eq!(
                        value.kind,
                        ExprKind::Literal(Literal::String("Hero".to_string()))
                    );
                }
                other => panic!("expected Named arg, got {other:?}"),
            }
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

// ── Trait declaration tests ──────────────────────────────────────────

#[test]
fn test_trait_decl() {
    let source = r#"trait Damageable {
        func takeDamage(amount: float)
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Trait(trait_decl) => {
            assert_eq!(trait_decl.name, "Damageable");
            assert_eq!(trait_decl.methods.len(), 1);
            assert_eq!(trait_decl.methods[0].name, "takeDamage");
            assert_eq!(trait_decl.methods[0].params.len(), 1);
            assert!(trait_decl.methods[0].default_body.is_none());
        }
        other => panic!("expected Trait, got {other:?}"),
    }
}

#[test]
fn test_trait_with_default() {
    let source = r#"trait Damageable {
        func takeDamage(amount: float)
        func die() {
            print("Entity died")
        }
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Trait(trait_decl) => {
            assert_eq!(trait_decl.methods.len(), 2);
            assert!(trait_decl.methods[0].default_body.is_none());
            assert!(trait_decl.methods[1].default_body.is_some());
            let body = trait_decl.methods[1].default_body.as_ref().unwrap();
            assert_eq!(body.len(), 1);
        }
        other => panic!("expected Trait, got {other:?}"),
    }
}

// ── Enum declaration tests ───────────────────────────────────────────

#[test]
fn test_enum_simple() {
    let stmt = parse_stmt("enum Direction { North, South, East, West }");
    match &stmt.kind {
        StmtKind::Enum(enum_decl) => {
            assert_eq!(enum_decl.name, "Direction");
            assert_eq!(enum_decl.variants.len(), 4);
            assert_eq!(enum_decl.variants[0].name, "North");
            assert_eq!(enum_decl.variants[1].name, "South");
            assert_eq!(enum_decl.variants[2].name, "East");
            assert_eq!(enum_decl.variants[3].name, "West");
            assert!(enum_decl.variants[0].value.is_none());
        }
        other => panic!("expected Enum, got {other:?}"),
    }
}

#[test]
fn test_enum_with_values() {
    let stmt = parse_stmt("enum Status { Alive(100), Dead(0), Wounded(50) }");
    match &stmt.kind {
        StmtKind::Enum(enum_decl) => {
            assert_eq!(enum_decl.name, "Status");
            assert_eq!(enum_decl.variants.len(), 3);
            assert_eq!(enum_decl.variants[0].name, "Alive");
            match &enum_decl.variants[0].value {
                Some(expr) => assert_eq!(expr.kind, ExprKind::Literal(Literal::Int(100))),
                None => panic!("expected value for Alive variant"),
            }
            assert_eq!(enum_decl.variants[1].name, "Dead");
            assert_eq!(enum_decl.variants[2].name, "Wounded");
        }
        other => panic!("expected Enum, got {other:?}"),
    }
}

#[test]
fn test_enum_with_methods() {
    let source = r#"enum Status {
        Alive(100), Dead(0), Wounded(50)

        health: int

        func getHealth() -> int {
            return health
        }
    }"#;
    let stmt = parse_stmt(source);
    match &stmt.kind {
        StmtKind::Enum(enum_decl) => {
            assert_eq!(enum_decl.name, "Status");
            assert_eq!(enum_decl.variants.len(), 3);
            assert_eq!(enum_decl.fields.len(), 1);
            assert_eq!(enum_decl.fields[0].name, "health");
            assert_eq!(enum_decl.methods.len(), 1);
            assert_eq!(enum_decl.methods[0].name, "getHealth");
        }
        other => panic!("expected Enum, got {other:?}"),
    }
}

// ── Import / Export tests ─────────────────────────────────────────────

#[test]
fn test_named_import() {
    let source = r#"import { Weapon, createWeapon } from "items/weapon""#;
    let decl = parse_decl(source);
    match &decl.kind {
        DeclKind::Import(import) => {
            assert_eq!(import.names, vec!["Weapon", "createWeapon"]);
            assert_eq!(import.from, "items/weapon");
        }
        other => panic!("expected Import, got {other:?}"),
    }
}

#[test]
fn test_wildcard_import() {
    let source = r#"import * as enemy from "entities/enemy""#;
    let decl = parse_decl(source);
    match &decl.kind {
        DeclKind::WildcardImport(import) => {
            assert_eq!(import.alias, "enemy");
            assert_eq!(import.from, "entities/enemy");
        }
        other => panic!("expected WildcardImport, got {other:?}"),
    }
}

#[test]
fn test_export_class() {
    let source = r#"export class Weapon {
        public damage: float = 10.0
    }"#;
    let decl = parse_decl(source);
    match &decl.kind {
        DeclKind::Export(inner) => match &inner.kind {
            DeclKind::Class(class) => {
                assert_eq!(class.name, "Weapon");
                assert_eq!(class.fields.len(), 1);
            }
            other => panic!("expected Class inside Export, got {other:?}"),
        },
        other => panic!("expected Export, got {other:?}"),
    }
}

#[test]
fn test_export_func() {
    let source = r#"export func createWeapon(damage: float) -> Weapon {
        return damage
    }"#;
    let decl = parse_decl(source);
    match &decl.kind {
        DeclKind::Export(inner) => match &inner.kind {
            DeclKind::Func(func) => {
                assert_eq!(func.name, "createWeapon");
                assert_eq!(func.params.len(), 1);
                assert_eq!(
                    func.return_type,
                    Some(TypeExpr::Simple("Weapon".to_string()))
                );
            }
            other => panic!("expected Func inside Export, got {other:?}"),
        },
        other => panic!("expected Export, got {other:?}"),
    }
}

// ── Full file integration test ────────────────────────────────────────

#[test]
fn test_full_file() {
    let source = r#"
import { Weapon } from "items/weapon"

export class Player extends Entity with Damageable {
    public name: string
    public health: float = 100.0
        set(value) { field = value }
    private speed: float = 5.0

    public func takeDamage(amount: float) {
        health -= amount
        if health <= 0 {
            die()
        }
    }

    public static func create(name: string) -> Player {
        return name
    }
}

trait Damageable {
    func takeDamage(amount: float)
    func die() {
        print("died")
    }
}

enum Direction {
    North, South, East, West
}

func main() {
    let player = Player(name: "Hero")
    let double = (x: float) => x * 2
    let (a, b) = (1, 2)
}
"#;
    let decls = parse_file(source);
    // Should have: import, export class, trait, enum, func
    assert_eq!(
        decls.len(),
        5,
        "expected 5 top-level declarations, got {}",
        decls.len()
    );
    assert!(matches!(&decls[0].kind, DeclKind::Import(_)));
    assert!(matches!(&decls[1].kind, DeclKind::Export(_)));
    assert!(matches!(&decls[2].kind, DeclKind::Trait(_)));
    assert!(matches!(&decls[3].kind, DeclKind::Enum(_)));
    assert!(matches!(&decls[4].kind, DeclKind::Func(_)));
}
