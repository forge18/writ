//! Integration tests for automatic module resolution.
//!
//! Each test writes `.writ` files to a temp directory and verifies that
//! `Writ::load()` automatically resolves and compiles imports.

use std::io::Write;

use writ::{Value, Writ};

/// Helper: write files to a temp dir and return the dir handle.
fn write_files(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    for (path, source) in files {
        let full_path = dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create dirs");
        }
        let mut f = std::fs::File::create(&full_path).expect("failed to create file");
        f.write_all(source.as_bytes())
            .expect("failed to write file");
    }
    dir
}

#[test]
fn basic_function_import() {
    let dir = write_files(&[
        (
            "math.writ",
            "export func add(a: int, b: int) -> int { return a + b }",
        ),
        (
            "main.writ",
            r#"
import { add } from "math"
return add(2, 3)
"#,
        ),
    ]);

    let mut writ = Writ::new();
    let main_path = dir.path().join("main.writ");
    writ.load(main_path.to_str().unwrap()).unwrap();
    let result = writ.call("add", &[Value::I32(10), Value::I32(20)]).unwrap();
    assert_eq!(result, Value::I32(30));
}

#[test]
fn transitive_imports() {
    let dir = write_files(&[
        ("a.writ", "export func baseValue() -> int { return 10 }"),
        (
            "b.writ",
            r#"
import { baseValue } from "a"
export func doubled() -> int { return baseValue() * 2 }
"#,
        ),
        (
            "main.writ",
            r#"
import { doubled } from "b"
return doubled()
"#,
        ),
    ]);

    let mut writ = Writ::new();
    let main_path = dir.path().join("main.writ");
    writ.load(main_path.to_str().unwrap()).unwrap();
    let result = writ.call("doubled", &[]).unwrap();
    assert_eq!(result, Value::I32(20));
}

#[test]
fn circular_import_detected() {
    let dir = write_files(&[
        (
            "a.writ",
            r#"
import { foo } from "b"
export func bar() -> int { return 1 }
"#,
        ),
        (
            "b.writ",
            r#"
import { bar } from "a"
export func foo() -> int { return 2 }
"#,
        ),
        (
            "main.writ",
            r#"
import { bar } from "a"
return bar()
"#,
        ),
    ]);

    let mut writ = Writ::new();
    let main_path = dir.path().join("main.writ");
    let result = writ.load(main_path.to_str().unwrap());
    assert!(result.is_err(), "circular import should produce an error");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("circular import"),
        "error should mention circular import, got: {err_msg}"
    );
}

#[test]
fn duplicate_import_loaded_once() {
    // Both b.writ and c.writ import a.writ. It should be compiled only once.
    let dir = write_files(&[
        ("a.writ", "export func shared() -> int { return 42 }"),
        (
            "b.writ",
            r#"
import { shared } from "a"
export func fromB() -> int { return shared() }
"#,
        ),
        (
            "c.writ",
            r#"
import { shared } from "a"
export func fromC() -> int { return shared() }
"#,
        ),
        (
            "main.writ",
            r#"
import { fromB } from "b"
import { fromC } from "c"
"#,
        ),
    ]);

    let mut writ = Writ::new();
    let main_path = dir.path().join("main.writ");
    writ.load(main_path.to_str().unwrap()).unwrap();

    let b = writ.call("fromB", &[]).unwrap();
    let c = writ.call("fromC", &[]).unwrap();
    assert_eq!(b, Value::I32(42));
    assert_eq!(c, Value::I32(42));
}

#[test]
fn missing_module_error() {
    let dir = write_files(&[("main.writ", r#"import { Foo } from "nonexistent""#)]);

    let mut writ = Writ::new();
    let main_path = dir.path().join("main.writ");
    let result = writ.load(main_path.to_str().unwrap());
    assert!(result.is_err(), "missing module should produce an error");
}

#[test]
fn nested_path_import() {
    let dir = write_files(&[
        (
            "items/weapon.writ",
            "export func damage() -> int { return 15 }",
        ),
        (
            "main.writ",
            r#"
import { damage } from "items/weapon"
return damage()
"#,
        ),
    ]);

    let mut writ = Writ::new();
    let main_path = dir.path().join("main.writ");
    writ.load(main_path.to_str().unwrap()).unwrap();
    let result = writ.call("damage", &[]).unwrap();
    assert_eq!(result, Value::I32(15));
}

#[test]
fn set_root_dir_resolution() {
    let dir = write_files(&[
        (
            "scripts/utils/math.writ",
            "export func square(n: int) -> int { return n * n }",
        ),
        (
            "scripts/main.writ",
            r#"
import { square } from "utils/math"
return square(5)
"#,
        ),
    ]);

    let mut writ = Writ::new();
    writ.set_root_dir(dir.path().join("scripts"));
    let main_path = dir.path().join("scripts/main.writ");
    writ.load(main_path.to_str().unwrap()).unwrap();
    let result = writ.call("square", &[Value::I32(7)]).unwrap();
    assert_eq!(result, Value::I32(49));
}

#[test]
fn backward_compatible_no_imports() {
    // A file with no imports should still work exactly as before.
    let dir = write_files(&[("simple.writ", "func greet() -> string { return \"hello\" }")]);

    let mut writ = Writ::new();
    let path = dir.path().join("simple.writ");
    writ.load(path.to_str().unwrap()).unwrap();
    let result = writ.call("greet", &[]).unwrap();
    assert_eq!(result, Value::Str("hello".into()));
}
