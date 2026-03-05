#![cfg(feature = "mobile-aosoa")]

use writ_compiler::Compiler;
use writ_lexer::Lexer;
use writ_parser::Parser;

use writ_vm::{VM, Value};

// ── Test helpers ────────────────────────────────────────────────────

fn eval(source: &str) -> Value {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();
    let mut vm = VM::new();
    vm.execute_program(&chunk, &functions, &struct_metas, &class_metas)
        .expect("vm failed")
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn test_aosoa_iterate_all() {
    let result = eval(
        r#"
        struct Point {
            public x: float = 0.0
            public y: float = 0.0
        }
        @packed var points = [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 4.0)]
        var sum = 0.0
        for p in points {
            sum = sum + p.x
        }
        return sum
        "#,
    );
    assert_eq!(result, Value::F32(4.0));
}

#[test]
fn test_aosoa_iterate_y_field() {
    let result = eval(
        r#"
        struct Point {
            public x: float = 0.0
            public y: float = 0.0
        }
        @packed var points = [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 4.0)]
        var sum = 0.0
        for p in points {
            sum = sum + p.y
        }
        return sum
        "#,
    );
    assert_eq!(result, Value::F32(6.0));
}

#[test]
fn test_aosoa_length() {
    let result = eval(
        r#"
        struct Point {
            public x: float = 0.0
            public y: float = 0.0
        }
        @packed var points = [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 4.0), Point(x: 5.0, y: 6.0)]
        return points.length
        "#,
    );
    assert_eq!(result, Value::I32(3));
}

#[test]
fn test_aosoa_for_loop_count() {
    let result = eval(
        r#"
        struct Point {
            public x: float = 0.0
            public y: float = 0.0
        }
        @packed var points = [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 4.0)]
        var count = 0
        for p in points {
            count += 1
        }
        return count
        "#,
    );
    assert_eq!(result, Value::I32(2));
}

#[test]
fn test_aosoa_chunk_boundary() {
    // Build an array with more than 64 elements to span multiple chunks.
    let mut source = String::from(
        r#"
        struct Item {
            public id: int = 0
        }
        @packed var items = [
        "#,
    );
    for i in 0..70 {
        if i > 0 {
            source.push_str(", ");
        }
        source.push_str(&format!("Item(id: {})", i));
    }
    source.push_str(
        r#"
        ]
        var last_id = 0
        for item in items {
            last_id = item.id
        }
        return last_id
        "#,
    );
    let result = eval(&source);
    assert_eq!(result, Value::I32(69));
}

#[test]
fn test_aosoa_chunk_boundary_sum() {
    // Verify all elements across chunk boundaries are accessible.
    let mut source = String::from(
        r#"
        struct Item {
            public id: int = 0
        }
        @packed var items = [
        "#,
    );
    for i in 0..70 {
        if i > 0 {
            source.push_str(", ");
        }
        source.push_str(&format!("Item(id: {})", i));
    }
    source.push_str(
        r#"
        ]
        var sum = 0
        for item in items {
            sum += item.id
        }
        return sum
        "#,
    );
    let result = eval(&source);
    // Sum of 0..70 = 69*70/2 = 2415
    assert_eq!(result, Value::I32(2415));
}

#[test]
fn test_aosoa_empty_array() {
    let result = eval(
        r#"
        struct Point {
            public x: float = 0.0
            public y: float = 0.0
        }
        @packed var points: Array<Point> = []
        return points.length
        "#,
    );
    assert_eq!(result, Value::I32(0));
}

#[test]
fn test_aosoa_single_element() {
    let result = eval(
        r#"
        struct Point {
            public x: float = 0.0
            public y: float = 0.0
        }
        @packed var points = [Point(x: 42.0, y: 99.0)]
        var result = 0.0
        for p in points {
            result = p.x
        }
        return result
        "#,
    );
    assert_eq!(result, Value::F32(42.0));
}
