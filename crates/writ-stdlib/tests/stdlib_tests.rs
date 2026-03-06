use std::rc::Rc;

use writ_compiler::Compiler;
use writ_lexer::Lexer;
use writ_parser::Parser;

use writ_vm::{RuntimeError, VM, Value};

// ── Test helpers ────────────────────────────────────────────────────

fn eval_with_stdlib(source: &str) -> Value {
    let mut vm = VM::new();
    writ_stdlib::register_all(&mut vm);
    eval_with_vm(source, &mut vm)
}

fn eval_error_with_stdlib(source: &str) -> RuntimeError {
    let mut vm = VM::new();
    writ_stdlib::register_all(&mut vm);
    eval_error_with_vm(source, &mut vm)
}

fn eval_with_vm(source: &str, vm: &mut VM) -> Value {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    for stmt in &stmts {
        compiler.compile_stmt(stmt).expect("compile failed");
    }
    let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();
    vm.execute_program(&chunk, &functions, &struct_metas, &class_metas)
        .expect("vm failed")
}

fn eval_error_with_vm(source: &str, vm: &mut VM) -> RuntimeError {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    for stmt in &stmts {
        compiler.compile_stmt(stmt).expect("compile failed");
    }
    let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();
    vm.execute_program(&chunk, &functions, &struct_metas, &class_metas)
        .expect_err("expected RuntimeError")
}

// ── Basic module tests ──────────────────────────────────────────────

#[test]
fn test_print_calls_host_output() {
    // print returns null — we can't easily capture eprintln output,
    // but we verify it doesn't error.
    let result = eval_with_stdlib(r#"print("hello")"#);
    assert_eq!(result, Value::Null);
}

#[test]
fn test_assert_passes() {
    let result = eval_with_stdlib(r#"assert(true, "ok")"#);
    assert_eq!(result, Value::Null);
}

#[test]
fn test_assert_fails() {
    let err = eval_error_with_stdlib(r#"assert(false, "boom")"#);
    assert!(err.message.contains("boom"), "error was: {}", err.message);
}

#[test]
fn test_type_returns_name() {
    assert_eq!(
        eval_with_stdlib("return type(42)"),
        Value::Str(Rc::new("int".to_string()))
    );
    assert_eq!(
        eval_with_stdlib(r#"return type("hello")"#),
        Value::Str(Rc::new("string".to_string()))
    );
    assert_eq!(
        eval_with_stdlib("return type(true)"),
        Value::Str(Rc::new("bool".to_string()))
    );
    assert_eq!(
        eval_with_stdlib("return type(3.14)"),
        Value::Str(Rc::new("float".to_string()))
    );
}

// ── Math module tests ───────────────────────────────────────────────

#[test]
fn test_math_abs() {
    assert_eq!(eval_with_stdlib("return abs(-5)"), Value::I32(5));
    assert_eq!(eval_with_stdlib("return abs(3)"), Value::I32(3));
}

#[test]
fn test_math_clamp() {
    assert_eq!(
        eval_with_stdlib("return clamp(10.0, 0.0, 5.0)"),
        Value::F64(5.0)
    );
    assert_eq!(
        eval_with_stdlib("return clamp(-3.0, 0.0, 5.0)"),
        Value::F64(0.0)
    );
    assert_eq!(
        eval_with_stdlib("return clamp(3.0, 0.0, 5.0)"),
        Value::F64(3.0)
    );
}

#[test]
fn test_math_sqrt() {
    let result = eval_with_stdlib("return sqrt(9.0)");
    match result {
        v @ (Value::F32(_) | Value::F64(_)) => {
            let v = v.as_f64();
            assert!((v - 3.0).abs() < 1e-10, "got {v}");
        }
        other => panic!("expected Float, got {other:?}"),
    }
}

#[test]
fn test_math_sin() {
    let result = eval_with_stdlib("return sin(0.0)");
    match result {
        v @ (Value::F32(_) | Value::F64(_)) => {
            let v = v.as_f64();
            assert!(v.abs() < 1e-10, "got {v}");
        }
        other => panic!("expected Float, got {other:?}"),
    }
}

#[test]
fn test_math_pi() {
    let result = eval_with_stdlib("return PI");
    match result {
        v @ (Value::F32(_) | Value::F64(_)) => {
            let v = v.as_f64();
            assert!((v - std::f64::consts::PI).abs() < 1e-10, "got {v}");
        }
        other => panic!("expected Float, got {other:?}"),
    }
}

// ── String module tests ─────────────────────────────────────────────

#[test]
fn test_string_len() {
    assert_eq!(eval_with_stdlib(r#"return "hello".len()"#), Value::I32(5));
}

#[test]
fn test_string_trim() {
    assert_eq!(
        eval_with_stdlib(r#"return " hello ".trim()"#),
        Value::Str(Rc::new("hello".to_string()))
    );
}

#[test]
fn test_string_split() {
    let result = eval_with_stdlib(r#"return "a,b,c".split(",")"#);
    match result {
        Value::Array(arr) => {
            let items = arr.borrow();
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::Str(Rc::new("a".to_string())));
            assert_eq!(items[1], Value::Str(Rc::new("b".to_string())));
            assert_eq!(items[2], Value::Str(Rc::new("c".to_string())));
        }
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn test_string_contains() {
    assert_eq!(
        eval_with_stdlib(r#"return "hello".contains("ell")"#),
        Value::Bool(true)
    );
    assert_eq!(
        eval_with_stdlib(r#"return "hello".contains("xyz")"#),
        Value::Bool(false)
    );
}

// ── Array module tests ──────────────────────────────────────────────

#[test]
fn test_array_push_pop() {
    let result = eval_with_stdlib(
        r#"
        let arr = [1, 2, 3]
        arr.push(4)
        return arr.pop()
        "#,
    );
    assert_eq!(result, Value::I32(4));
}

#[test]
fn test_array_map() {
    let result = eval_with_stdlib(
        r#"
        func double(x: int) -> int {
            return x * 2
        }
        let arr = [1, 2, 3]
        return arr.map(double)
        "#,
    );
    match result {
        Value::Array(arr) => {
            let items = arr.borrow();
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::I32(2));
            assert_eq!(items[1], Value::I32(4));
            assert_eq!(items[2], Value::I32(6));
        }
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn test_array_filter() {
    let result = eval_with_stdlib(
        r#"
        func is_even(x: int) -> bool {
            return x % 2 == 0
        }
        let arr = [1, 2, 3, 4]
        return arr.filter(is_even)
        "#,
    );
    match result {
        Value::Array(arr) => {
            let items = arr.borrow();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], Value::I32(2));
            assert_eq!(items[1], Value::I32(4));
        }
        other => panic!("expected Array, got {other:?}"),
    }
}

// ── Dictionary module tests ─────────────────────────────────────────

#[test]
fn test_dictionary_keys() {
    let result = eval_with_stdlib(
        r#"
        let d = {"a": 1, "b": 2}
        return d.keys()
        "#,
    );
    match result {
        Value::Array(arr) => {
            let items = arr.borrow();
            assert_eq!(items.len(), 2);
            // Keys may be in any order
            let has_a = items
                .iter()
                .any(|v| v == &Value::Str(Rc::new("a".to_string())));
            let has_b = items
                .iter()
                .any(|v| v == &Value::Str(Rc::new("b".to_string())));
            assert!(has_a, "missing key 'a'");
            assert!(has_b, "missing key 'b'");
        }
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn test_dictionary_merge() {
    let result = eval_with_stdlib(
        r#"
        let a = {"x": 1}
        let b = {"y": 2}
        a.merge(b)
        return a.len()
        "#,
    );
    assert_eq!(result, Value::I32(2));
}

// ── I/O module tests ────────────────────────────────────────────────

#[test]
fn test_io_write_read_roundtrip() {
    let dir = std::env::temp_dir();
    let path = dir.join("writ_test_io.txt");
    let path_str = path.to_str().unwrap();

    let result = eval_with_stdlib(&format!(
        r#"
        writeFile("{path_str}", "hello writ")
        return readFile("{path_str}")
        "#,
    ));
    assert_eq!(result, Value::Str(Rc::new("hello writ".to_string())));

    // Cleanup
    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_io_disabled_when_module_excluded() {
    let mut vm = VM::new();
    writ_stdlib::register_except(&mut vm, &["io"]);

    let err = eval_error_with_vm(r#"return readFile("test.txt")"#, &mut vm);
    assert!(
        err.message.contains("undefined function") || err.message.contains("disabled"),
        "error was: {}",
        err.message
    );
}

// ── Random module tests ─────────────────────────────────────────────

#[test]
fn test_random_in_range() {
    for _ in 0..20 {
        let result = eval_with_stdlib("return randomInt(1, 10)");
        match result {
            v @ (Value::I32(_) | Value::I64(_)) => {
                let v = v.as_i64();
                assert!((1..=10).contains(&v), "got {v}");
            }
            other => panic!("expected Int, got {other:?}"),
        }
    }
}

#[test]
fn test_shuffle_changes_order() {
    // Shuffle an array and verify it changed by checking first/last elements
    // or using reduce to compute a weighted sum that depends on ordering.
    // The chance of a shuffle returning a sorted [1..20] is 1/20! ≈ 4e-19.
    let result = eval_with_stdlib(
        r#"
        let arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]
        shuffle(arr)
        return arr.first() == 1 && arr.last() == 20
        "#,
    );
    // Very unlikely both first AND last stay in original position after shuffle
    assert_eq!(result, Value::Bool(false));
}
