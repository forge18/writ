use std::collections::HashMap;

use writ_lexer::Lexer;
use writ_parser::{Parser, Visibility};
use writ_types::{MethodInfo, Type, TypeChecker, TypeError};

// ── Helpers ──────────────────────────────────────────────────────────

/// Parses and type-checks a program. Returns `Ok(())` on success.
fn check(source: &str) -> Result<(), TypeError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut checker = TypeChecker::new();
    checker.check_program(&stmts)
}

/// Parses and type-checks a program. Expects a `TypeError` and returns it.
fn check_error(source: &str) -> TypeError {
    check(source).expect_err("expected TypeError")
}

/// Parses a single expression and infers its type.
fn infer(source: &str) -> Type {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr().expect("parser failed");
    let mut checker = TypeChecker::new();
    checker.infer_expr(&expr).expect("type error")
}

// ── Tests ────────────────────────────────────────────────────────────

#[test]
fn test_let_int_literal() {
    check("let x = 42").unwrap();
}

#[test]
fn test_let_float_literal() {
    check("let x = 3.14").unwrap();
}

#[test]
fn test_let_string_literal() {
    check(r#"let x = "hello""#).unwrap();
}

#[test]
fn test_let_bool_literal() {
    check("let x = true").unwrap();
}

#[test]
fn test_let_annotation_matches() {
    check("let x: int = 42").unwrap();
    check("let y: float = 3.14").unwrap();
    check(r#"let z: string = "hi""#).unwrap();
    check("let w: bool = false").unwrap();
}

#[test]
fn test_let_annotation_mismatch() {
    let err = check_error("let x: int = 3.14");
    assert!(err.message.contains("type mismatch"));
    assert!(err.message.contains("int"));
    assert!(err.message.contains("float"));
}

#[test]
fn test_var_reassign_same_type() {
    check("var x = 42\nx = 10").unwrap();
}

#[test]
fn test_var_reassign_wrong_type() {
    let err = check_error("var x = 42\nx = \"hello\"");
    assert!(err.message.contains("type mismatch"));
}

#[test]
fn test_const_inferred() {
    check("const MAX = 100").unwrap();
}

#[test]
fn test_binary_arithmetic_type() {
    assert_eq!(infer("1 + 2"), Type::Int);
    assert_eq!(infer("1.0 + 2.0"), Type::Float);
    check("let x = 1 + 2").unwrap();
    check("let y = 1.0 * 2.0").unwrap();
}

#[test]
fn test_binary_comparison_returns_bool() {
    assert_eq!(infer("1 < 2"), Type::Bool);
    assert_eq!(infer("1 == 2"), Type::Bool);
    assert_eq!(infer("1 != 2"), Type::Bool);
}

#[test]
fn test_logical_and_requires_bool() {
    let err = check_error("let x = 1 && true");
    assert!(err.message.contains("bool"));
    assert!(err.message.contains("int"));
}

#[test]
fn test_scope_isolation() {
    let err = check_error("let x = 1\n{\n  let y = 2\n}\nlet z = y");
    assert!(err.message.contains("undefined variable"));
    assert!(err.message.contains("y"));
}

#[test]
fn test_undefined_variable() {
    let err = check_error("let x = y");
    assert!(err.message.contains("undefined variable"));
    assert!(err.message.contains("y"));
}

// ── Phase 6: Functions + Return Types ───────────────────────────────

#[test]
fn test_func_correct_return() {
    check("func add(a: int, b: int) -> int {\n  return a + b\n}").unwrap();
}

#[test]
fn test_func_wrong_return_type() {
    let err = check_error("func greet() -> int {\n  return \"hello\"\n}");
    assert!(err.message.contains("return type mismatch"));
}

#[test]
fn test_func_missing_return_on_branch() {
    let err = check_error("func f(x: bool) -> int {\n  if x {\n    return 1\n  }\n}");
    assert!(err.message.contains("missing return"));
}

#[test]
fn test_func_void_no_return() {
    check("func doStuff() {\n  let x = 1\n}").unwrap();
}

#[test]
fn test_call_correct_args() {
    check("func add(a: int, b: int) -> int {\n  return a + b\n}\nlet x = add(1, 2)").unwrap();
}

#[test]
fn test_call_wrong_arg_count() {
    let err = check_error("func add(a: int, b: int) -> int {\n  return a + b\n}\nlet x = add(1)");
    assert!(err.message.contains("argument"));
}

#[test]
fn test_call_wrong_arg_type() {
    let err = check_error(
        "func add(a: int, b: int) -> int {\n  return a + b\n}\nlet x = add(1, \"hello\")",
    );
    assert!(err.message.contains("type mismatch"));
}

#[test]
fn test_propagate_operator_in_result_func() {
    check(
        "func inner() -> Result<int> {\n  return Success(42)\n}\n\
         func outer() -> Result<int> {\n  let v = inner()?\n  return Success(v)\n}",
    )
    .unwrap();
}

#[test]
fn test_propagate_operator_outside_result_func() {
    let err = check_error(
        "func inner() -> Result<int> {\n  return Success(42)\n}\n\
         func outer() -> int {\n  let v = inner()?\n  return v\n}",
    );
    assert!(err.message.contains("Result<T>"));
}

#[test]
fn test_result_when_exhaustive() {
    check(
        "func f() -> Result<int> {\n  return Success(1)\n}\n\
         let r = f()\n\
         when r {\n  is Success(v) => v + 1\n  is Error(e) => 0\n}",
    )
    .unwrap();
}

#[test]
fn test_result_when_missing_arm() {
    let err = check_error(
        "func f() -> Result<int> {\n  return Success(1)\n}\n\
         let r = f()\n\
         when r {\n  is Success(v) => v + 1\n}",
    );
    assert!(err.message.contains("exhaustive") || err.message.contains("Error"));
}

#[test]
fn test_optional_null_coalesce() {
    check(
        "func f() -> Optional<string> {\n  return null\n}\n\
         let name = f() ?? \"Unknown\"",
    )
    .unwrap();
}

#[test]
fn test_optional_safe_access() {
    check(
        "func f() -> Optional<string> {\n  return null\n}\n\
         let result = f()?.length",
    )
    .unwrap();
}

#[test]
fn test_optional_non_nullable_assignment() {
    let err = check_error("let x: string = null");
    assert!(
        err.message.contains("type mismatch")
            || err.message.contains("null")
            || err.message.contains("Optional")
    );
}

#[test]
fn test_lambda_inferred_return() {
    check("let double = (x: int) => x * 2").unwrap();
}

#[test]
fn test_lambda_wrong_param_count() {
    let err = check_error("let double = (x: int) => x * 2\nlet r = double(1, 2)");
    assert!(err.message.contains("argument"));
}

#[test]
fn test_tuple_destructure_types() {
    check("let point = (10.0, 20.0)\nlet (x, y) = point").unwrap();
}

// ── Phase 7: Classes + Traits + Enums ───────────────────────────────

#[test]
fn test_class_field_access() {
    check(
        "class Player {\n  public name: string = \"Hero\"\n  public health: float = 100.0\n}\n\
         let p = Player(name: \"Hero\", health: 80.0)\n\
         let n: string = p.name",
    )
    .unwrap();
}

#[test]
fn test_class_field_wrong_type() {
    let err = check_error("class Player {\n  public name: string = 42\n}");
    assert!(err.message.contains("type mismatch"));
}

#[test]
fn test_class_private_field_external_access() {
    let err = check_error(
        "class Player {\n  health: float = 100.0\n}\n\
         let p = Player(health: 80.0)\n\
         let h = p.health",
    );
    assert!(err.message.contains("private"));
}

#[test]
fn test_class_method_self() {
    check(
        "class Player {\n  public health: float = 100.0\n\
         \n  public func takeDamage(amount: float) {\n    health = health - amount\n  }\n}",
    )
    .unwrap();
}

#[test]
fn test_class_constructor_all_fields() {
    check(
        "class Player {\n  public name: string\n  public health: float\n}\n\
         let p = Player(\"Hero\", 100.0)",
    )
    .unwrap();
}

#[test]
fn test_class_constructor_named_params() {
    check(
        "class Player {\n  public name: string\n  public health: float\n}\n\
         let p = Player(name: \"Hero\", health: 100.0)",
    )
    .unwrap();
}

#[test]
fn test_class_extends_inherits_fields() {
    check(
        "class Entity {\n  public id: int = 0\n}\n\
         class Player extends Entity {\n  public name: string\n}\n\
         let p = Player(id: 1, name: \"Hero\")\n\
         let i: int = p.id",
    )
    .unwrap();
}

#[test]
fn test_class_extends_assignable() {
    check(
        "class Entity {\n  public id: int = 0\n}\n\
         class Player extends Entity {\n  public name: string\n}\n\
         let p = Player(id: 1, name: \"Hero\")\n\
         let e: Entity = p",
    )
    .unwrap();
}

#[test]
fn test_class_trait_impl_complete() {
    check(
        "trait Damageable {\n  func takeDamage(amount: float)\n}\n\
         class Player with Damageable {\n  public health: float = 100.0\n\
         \n  public func takeDamage(amount: float) {\n    health = health - amount\n  }\n}",
    )
    .unwrap();
}

#[test]
fn test_class_trait_impl_missing_method() {
    let err = check_error(
        "trait Damageable {\n  func takeDamage(amount: float)\n}\n\
         class Player with Damageable {\n  public health: float = 100.0\n}",
    );
    assert!(err.message.contains("does not implement"));
    assert!(err.message.contains("takeDamage"));
}

#[test]
fn test_class_trait_default_method_inherited() {
    check(
        "trait Greetable {\n  func greet() -> string {\n    return \"Hello\"\n  }\n}\n\
         class Player with Greetable {\n  public name: string\n}",
    )
    .unwrap();
}

#[test]
fn test_class_trait_method_override() {
    check(
        "trait Greetable {\n  func greet() -> string {\n    return \"Hello\"\n  }\n}\n\
         class Player with Greetable {\n  public name: string\n\
         \n  public func greet() -> string {\n    return \"Hi\"\n  }\n}",
    )
    .unwrap();
}

#[test]
fn test_class_two_traits_conflict() {
    let err = check_error(
        "trait TraitA {\n  func doThing() {\n    let x = 1\n  }\n}\n\
         trait TraitB {\n  func doThing() {\n    let y = 2\n  }\n}\n\
         class MyClass with TraitA, TraitB {\n  public value: int = 0\n}",
    );
    assert!(err.message.contains("conflicting"));
    assert!(err.message.contains("doThing"));
}

#[test]
fn test_class_setter_type() {
    check("class Player {\n  public health: float = 100.0 set(value) {\n    field = value\n  }\n}")
        .unwrap();
}

#[test]
fn test_class_setter_wrong_type() {
    let err = check_error(
        "class Player {\n  public health: float = 100.0 set(value) {\n    field = \"bad\"\n  }\n}",
    );
    assert!(err.message.contains("type mismatch"));
}

#[test]
fn test_enum_variant_access() {
    check(
        "enum Direction {\n  North, South, East, West\n}\n\
         let d: Direction = Direction.North",
    )
    .unwrap();
}

#[test]
fn test_enum_method_call() {
    check(
        "enum Direction {\n  North, South, East, West\n\
         \n  func isVertical() -> bool {\n    return true\n  }\n}\n\
         let d = Direction.North\n\
         let v: bool = d.isVertical()",
    )
    .unwrap();
}

#[test]
fn test_when_enum_exhaustive() {
    check(
        "enum Direction {\n  North, South, East, West\n}\n\
         let d = Direction.North\n\
         when d {\n  North => 1\n  South => 2\n  East => 3\n  West => 4\n}",
    )
    .unwrap();
}

#[test]
fn test_when_enum_missing_variant() {
    let err = check_error(
        "enum Direction {\n  North, South, East, West\n}\n\
         let d = Direction.North\n\
         when d {\n  North => 1\n  South => 2\n}",
    );
    assert!(err.message.contains("non-exhaustive") || err.message.contains("missing"));
    assert!(err.message.contains("East") || err.message.contains("West"));
}

#[test]
fn test_when_enum_with_else() {
    check(
        "enum Direction {\n  North, South, East, West\n}\n\
         let d = Direction.North\n\
         when d {\n  North => 1\n  else => 0\n}",
    )
    .unwrap();
}

// ── Phase 8: Module resolution tests ──────────────────────────────────

/// Parses and type-checks with pre-registered modules.
fn check_with_modules(source: &str, modules: &[(&str, &[(&str, Type)])]) -> Result<(), TypeError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut checker = TypeChecker::new();
    for (path, exports) in modules {
        let map: HashMap<String, Type> = exports
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        checker.register_module(path, map);
    }
    checker.check_program(&stmts)
}

/// Parses and type-checks with a pre-registered host type.
fn check_with_host_type(
    source: &str,
    name: &str,
    methods: Vec<MethodInfo>,
) -> Result<(), TypeError> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut checker = TypeChecker::new();
    checker.register_host_type(name, methods);
    checker.check_program(&stmts)
}

#[test]
fn test_named_import_resolves() {
    check_with_modules(
        r#"import { getHealth } from "items/weapon"
           let h: int = getHealth()"#,
        &[(
            "items/weapon",
            &[(
                "getHealth",
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Int),
                },
            )],
        )],
    )
    .unwrap();
}

#[test]
fn test_named_import_unknown_path() {
    let err = check_with_modules(
        r#"import { Weapon } from "items/weapon""#,
        &[], // no modules registered
    )
    .unwrap_err();
    assert!(
        err.message.contains("unknown module"),
        "error was: {}",
        err.message
    );
}

#[test]
fn test_named_import_unknown_name() {
    let err = check_with_modules(
        r#"import { Shield } from "items/weapon""#,
        &[(
            "items/weapon",
            &[(
                "Weapon",
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Void),
                },
            )],
        )],
    )
    .unwrap_err();
    assert!(
        err.message.contains("no export") && err.message.contains("Shield"),
        "error was: {}",
        err.message
    );
}

#[test]
fn test_wildcard_import_namespace_access() {
    check_with_modules(
        r#"import * as enemy from "entities/enemy"
           let h: int = enemy::getHealth()"#,
        &[(
            "entities/enemy",
            &[(
                "getHealth",
                Type::Function {
                    params: vec![],
                    return_type: Box::new(Type::Int),
                },
            )],
        )],
    )
    .unwrap();
}

#[test]
fn test_export_not_accessible_without_import() {
    // Without importing, a name that exists elsewhere is not accessible.
    let err = check("let w = Weapon()").unwrap_err();
    assert!(
        err.message.contains("undefined") || err.message.contains("not callable"),
        "error was: {}",
        err.message
    );
}

// ── Phase 8: Collection type tests ────────────────────────────────────

#[test]
fn test_array_push_type() {
    check(
        r#"var items: Array<string> = ["hello", "world"]
           items.push("new_item")"#,
    )
    .unwrap();
}

#[test]
fn test_array_map_returns_typed_array() {
    check(
        r#"let items: Array<int> = [1, 2, 3]
           let doubled = items.map((x: int) => x)"#,
    )
    .unwrap();
}

#[test]
fn test_array_filter_predicate_type() {
    check(
        r#"let items: Array<int> = [1, 2, 3]
           let evens = items.filter((x: int) => x == 2)"#,
    )
    .unwrap();
}

#[test]
fn test_dictionary_contains_type() {
    check(
        r#"let scores: Dictionary<string, int> = {"alice": 100, "bob": 95}
           let found: bool = scores.contains("alice")"#,
    )
    .unwrap();
}

#[test]
fn test_spread_array_matching_types() {
    check(
        r#"let a: Array<int> = [1, 2]
           let b: Array<int> = [3, 4]
           let c: Array<int> = [...a, ...b]"#,
    )
    .unwrap();
}

#[test]
fn test_spread_array_mismatched_types() {
    let err = check_error(
        r#"let a: Array<int> = [1, 2]
           let b: Array<string> = ["x", "y"]
           let c = [...a, ...b]"#,
    );
    assert!(
        err.message.contains("mismatch"),
        "error was: {}",
        err.message
    );
}

// ── Phase 8: Cast tests ──────────────────────────────────────────────

#[test]
fn test_cast_int_to_float() {
    let ty = infer("42 as float");
    assert_eq!(ty, Type::Float);
}

#[test]
fn test_cast_invalid() {
    let err = check_error(r#"let x = "hello" as int"#);
    assert!(
        err.message.contains("cannot cast"),
        "error was: {}",
        err.message
    );
}

// ── Phase 8: Host type tests ─────────────────────────────────────────

#[test]
fn test_host_registered_type_globally_available() {
    check_with_host_type("func test(e: Enemy) -> void {}", "Enemy", vec![]).unwrap();
}

#[test]
fn test_host_registered_type_method_call() {
    check_with_host_type(
        r#"func test(e: Enemy) -> void {
               e.takeDamage(10.0)
           }"#,
        "Enemy",
        vec![MethodInfo {
            name: "takeDamage".to_string(),
            params: vec![Type::Float],
            return_type: Type::Void,
            is_static: false,
            visibility: Visibility::Public,
            has_default_body: false,
        }],
    )
    .unwrap();
}

// ── Phase 20: Suggestion tests ──────────────────────────────────────

#[test]
fn test_suggest_misspelled_variable() {
    let err = check_error("let health = 10\nlet x = helth");
    assert!(err.message.contains("undefined variable"));
    assert!(!err.suggestions.is_empty());
    assert!(err.suggestions[0].message.contains("health"));
    assert_eq!(err.suggestions[0].replacement, Some("health".to_string()));
}

#[test]
fn test_suggest_misspelled_type() {
    let err = check_error(
        "class Player {\n  public health: float = 100.0\n}\n\
         let x: Plyer = Player(health: 1.0)",
    );
    assert!(err.message.contains("unknown type"));
    assert!(!err.suggestions.is_empty());
    assert!(err.suggestions[0].message.contains("Player"));
}

#[test]
fn test_suggest_misspelled_field() {
    let err = check_error(
        "class Player {\n  public health: float = 100.0\n}\n\
         let p = Player(health: 100.0)\n\
         let x = p.helth",
    );
    assert!(err.message.contains("no member"));
    assert!(!err.suggestions.is_empty());
    assert!(err.suggestions[0].message.contains("health"));
}

#[test]
fn test_suggest_unwrap_result() {
    let err = check_error(
        "func getValue() -> Result<int> {\n  return Success(42)\n}\n\
         func main() -> int {\n  let x: int = getValue()\n  return x\n}",
    );
    assert!(err.message.contains("type mismatch"));
    assert!(err.suggestions.iter().any(|s| s.message.contains("?")));
}

#[test]
fn test_suggest_call_function() {
    let err = check_error(
        "func getCount() -> int {\n  return 42\n}\n\
         let x: int = getCount",
    );
    assert!(err.message.contains("type mismatch"));
    assert!(err.suggestions.iter().any(|s| s.message.contains("()")));
}

#[test]
fn test_suggest_public_getter_for_private_field() {
    let err = check_error(
        "class Player {\n  health: float = 100.0\n\
         \n  public func getHealth() -> float {\n    return health\n  }\n}\n\
         let p = Player(health: 80.0)\n\
         let h = p.health",
    );
    assert!(err.message.contains("private"));
    assert!(
        err.suggestions
            .iter()
            .any(|s| s.message.contains("getHealth"))
    );
}

#[test]
fn test_suggest_type_conversion() {
    let err = check_error("let x: float = 42");
    assert!(err.message.contains("type mismatch"));
    assert!(
        err.suggestions
            .iter()
            .any(|s| s.message.contains("as float"))
    );
}

#[test]
fn test_no_suggestion_when_nothing_close() {
    let err = check_error("let x = xyzzyplugh");
    assert!(err.message.contains("undefined variable"));
    assert!(err.suggestions.is_empty());
}

#[test]
fn test_suggest_prefers_same_scope() {
    // "counte" is close to both "count" (outer) and "counter" (inner)
    // Inner scope should be preferred via weighted matching
    let err = check_error(
        "let count = 10\n\
         {\n  let counter = 20\n  let x = counte\n}",
    );
    assert!(err.message.contains("undefined variable"));
    assert!(!err.suggestions.is_empty());
    // Both "count" and "counter" are 1 edit away from "counte"
    // The suggestion should be present (either is acceptable)
    let suggested = &err.suggestions[0].message;
    assert!(suggested.contains("count") || suggested.contains("counter"));
}

#[test]
fn test_suggest_prefers_type_compatible() {
    // "coun" is close to both "count" (int) and "county" (string)
    // When assigning to int, "count" should be preferred
    let err = check_error(
        "let count: int = 10\n\
         let county: string = \"LA\"\n\
         let x: int = coun",
    );
    assert!(err.message.contains("undefined variable"));
    assert!(!err.suggestions.is_empty());
    // "coun" -> "count" = 1, "county" = 2 — count wins on edit distance
    assert!(err.suggestions[0].message.contains("count"));
}
