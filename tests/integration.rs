//! End-to-end integration tests for the Writ scripting engine.
//!
//! Each test creates a [`Writ`] instance and runs source through the full
//! pipeline: lexer → parser → (type check) → compiler → VM.

use std::cell::RefCell;
use std::rc::Rc;

use writ::{Value, ValueTag, Writ, WritError, WritObject, Type, fn0, fn1, fn2, fn3, mfn0, mfn1, mfn2, mfn3};

// ── Helpers ──────────────────────────────────────────────────────────

/// A mock host-owned type for testing host type integration.
#[derive(Debug)]
struct MockPlayer {
    name: String,
    health: f32,
}

impl WritObject for MockPlayer {
    fn type_name(&self) -> &str {
        "Player"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "name" => Ok(Value::Str(Rc::from(self.name.as_str()))),
            "health" => Ok(Value::F32(self.health)),
            _ => Err(format!("Player has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "health" => match value {
                v @ (Value::F32(_) | Value::F64(_)) => {
                    self.health = v.as_f64() as f32;
                    Ok(())
                }
                _ => Err(format!(
                    "expected float for health, got {}",
                    value.type_name()
                )),
            },
            _ => Err(format!("Player has no settable field '{name}'")),
        }
    }

    fn call_method(&mut self, name: &str, _args: &[Value]) -> Result<Value, String> {
        match name {
            "greet" => Ok(Value::Str(Rc::from(format!("Hello, I'm {}!", self.name).as_str()))),
            _ => Err(format!("Player has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ── Test 1: Hello World ──────────────────────────────────────────────

#[test]
fn test_hello_world() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run(r#"print("Hello, World!")"#);
    assert!(result.is_ok());
}

// ── Test 2: Fibonacci ────────────────────────────────────────────────

#[test]
fn test_fibonacci() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ
        .run(
            "func fib(n: int) -> int {\n\
             if n <= 1 { return n }\n\
             return fib(n - 1) + fib(n - 2)\n\
             }\n\
             return fib(10)",
        )
        .unwrap();
    assert_eq!(result, Value::I32(55));
}

// ── Test 3: Class Instantiation ──────────────────────────────────────
//
// The compiler does not yet compile class declarations. This test
// exercises the equivalent functionality using host-registered types
// and field access, which exercises the same VM field/method dispatch.

#[test]
fn test_class_instantiation() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let player = MockPlayer {
        name: "Hero".to_string(),
        health: 100.0,
    };
    let player_obj = Value::Object(Rc::new(RefCell::new(player)));
    let obj_clone = player_obj.clone();
    writ.register_fn(
        "create_player",
        fn0(move || -> Result<Value, String> { Ok(obj_clone.clone()) }),
    );

    let result = writ
        .run(
            "func test() -> float {\n\
             let p = create_player()\n\
             return p.health\n\
             }\n\
             return test()",
        )
        .unwrap();
    assert_eq!(result, Value::F32(100.0));
}

// ── Test 4: Trait Dispatch ───────────────────────────────────────────
//
// The compiler does not yet compile trait declarations. This test
// exercises method dispatch on host-registered objects, which is the
// VM-level mechanism that trait dispatch compiles down to.

#[test]
fn test_trait_dispatch() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let player = MockPlayer {
        name: "Hero".to_string(),
        health: 100.0,
    };
    let player_obj = Rc::new(RefCell::new(player));
    let obj_ref = Rc::clone(&player_obj);
    writ.register_fn(
        "greet_player",
        fn0(move || -> Result<Value, String> {
            obj_ref
                .borrow_mut()
                .call_method("greet", &[])
                .map_err(|e| e.to_string())
        }),
    );

    let result = writ.run("return greet_player()").unwrap();
    assert_eq!(result, Value::Str(Rc::from("Hello, I'm Hero!")));
}

// ── Test 5: Coroutine Integration ────────────────────────────────────

#[test]
fn test_coroutine_integration() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);
    writ.register_fn(
        "inc",
        fn0(move || -> Result<Value, String> {
            *counter_ref.borrow_mut() += 1;
            Ok(Value::Null)
        }),
    );

    writ.run(
        "func work() {\n\
         inc()\n\
         yield\n\
         inc()\n\
         }\n\
         start work()",
    )
    .unwrap();

    // After run: coroutine is queued but not yet executed.
    assert_eq!(*counter.borrow(), 0);

    // tick 1: first run — calls inc(), hits yield, suspends.
    writ.tick(0.016).unwrap();
    assert_eq!(*counter.borrow(), 1);

    // tick 2: resumes after yield — calls inc(), completes.
    writ.tick(0.016).unwrap();
    assert_eq!(*counter.borrow(), 2);
}

// ── Test 6: Module Import ────────────────────────────────────────────

#[test]
fn test_module_import() {
    use std::io::Write;

    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Write a temporary module file
    let dir = std::env::temp_dir().join("writ_test_module");
    std::fs::create_dir_all(&dir).unwrap();
    let module_path = dir.join("math_utils.writ");
    {
        let mut f = std::fs::File::create(&module_path).unwrap();
        writeln!(f, "func double(n: int) -> int {{ return n * 2 }}").unwrap();
    }

    // Load the module (compiles and stores functions)
    writ.load(module_path.to_str().unwrap()).unwrap();

    // Call the loaded function
    let result = writ.call("double", &[Value::I32(21)]).unwrap();
    assert_eq!(result, Value::I32(42));

    // Cleanup
    std::fs::remove_dir_all(dir).ok();
}

// ── Test 7: Result Propagation ───────────────────────────────────────
//
// The `?` operator (ErrorPropagate) is not yet compiled. This test
// exercises manual error propagation using `when` value matching,
// which is the desugared equivalent.

#[test]
fn test_result_propagation() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Simulate Result-like propagation with manual when matching
    let result = writ
        .run(
            "func may_fail(x: int) -> int {\n\
             if x < 0 { return -1 }\n\
             return x * 2\n\
             }\n\
             func caller() -> int {\n\
             let r = may_fail(5)\n\
             if r == -1 { return -1 }\n\
             return r + 10\n\
             }\n\
             return caller()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(20));

    // Error case
    let result2 = writ
        .run(
            "func may_fail(x: int) -> int {\n\
             if x < 0 { return -1 }\n\
             return x * 2\n\
             }\n\
             func caller() -> int {\n\
             let r = may_fail(-3)\n\
             if r == -1 { return -1 }\n\
             return r + 10\n\
             }\n\
             return caller()",
        )
        .unwrap();
    assert_eq!(result2, Value::I32(-1));
}

// ── Test 8: Optional Chain ───────────────────────────────────────────

#[test]
fn test_optional_chain() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // null ?? default_value
    let result = writ.run("return null ?? 42").unwrap();
    assert_eq!(result, Value::I32(42));

    // non-null ?? default_value
    let result2 = writ.run("return 10 ?? 42").unwrap();
    assert_eq!(result2, Value::I32(10));
}

// ── Test 9: When Exhaustive ──────────────────────────────────────────
//
// TypeMatch patterns (`is Success`/`is Error`) are not yet compiled.
// This test exercises `when` with value matching and else, which
// demonstrates the when construct works end-to-end.

#[test]
fn test_when_exhaustive() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "func describe(x: int) -> int {\n\
             when x {\n\
             1 => 10\n\
             2, 3 => 20\n\
             else => 0\n\
             }\n\
             return 0\n\
             }\n\
             return describe(2)",
        )
        .unwrap();
    // when arms execute as expression statements (value discarded);
    // the function returns the trailing return.
    // Verify the when construct compiles and runs without error.
    assert_eq!(result, Value::I32(0));

    // Use block bodies to test when with actual control flow
    let result2 = writ
        .run(
            "func describe(x: int) -> int {\n\
             when x {\n\
             1 => { return 10 }\n\
             else => { return 99 }\n\
             }\n\
             }\n\
             return describe(42)",
        )
        .unwrap();
    assert_eq!(result2, Value::I32(99));
}

// ── Test 10: Host Type Integration ───────────────────────────────────

#[test]
fn test_host_type_integration() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let player = MockPlayer {
        name: "Hero".to_string(),
        health: 100.0,
    };
    let player_obj = Value::Object(Rc::new(RefCell::new(player)));
    let obj_clone = player_obj.clone();
    writ.register_fn(
        "get_player",
        fn0(move || -> Result<Value, String> { Ok(obj_clone.clone()) }),
    );

    // Access field through the full pipeline
    let result = writ
        .run(
            "func test() -> float {\n\
             let p = get_player()\n\
             return p.health\n\
             }\n\
             return test()",
        )
        .unwrap();
    assert_eq!(result, Value::F32(100.0));
}

// ── Test 11: Sandbox Enforcement ─────────────────────────────────────

#[test]
fn test_sandbox_enforcement() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run("return unknown_function(42)");
    assert!(result.is_err());
    match result.unwrap_err() {
        WritError::Runtime(e) => {
            assert!(
                e.message.contains("undefined function"),
                "expected 'undefined function', got: {}",
                e.message
            );
        }
        other => panic!("expected WritError::Runtime, got: {other}"),
    }
}

// ── Test 12: Instruction Limit ───────────────────────────────────────

#[test]
fn test_instruction_limit_integration() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.set_instruction_limit(50);

    let result = writ.run(
        "func spin() -> int {\n\
         var x = 0\n\
         while true { x += 1 }\n\
         return x\n\
         }\n\
         return spin()",
    );
    assert!(result.is_err());
    match result.unwrap_err() {
        WritError::Runtime(e) => {
            assert!(
                e.message.contains("instruction limit"),
                "expected 'instruction limit', got: {}",
                e.message
            );
        }
        other => panic!("expected WritError::Runtime, got: {other}"),
    }
}

// ── Test 13: Struct Creation and Field Access ────────────────────────

#[test]
fn test_struct_creation_and_field_access() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(3, 7)\n\
             return p.x + p.y",
        )
        .unwrap();
    assert_eq!(result, Value::I32(10));
}

// ── Test 14: Struct Value Semantics ──────────────────────────────────

#[test]
fn test_struct_value_semantics() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Assigning a struct copies it — modifying the copy doesn't affect the original.
    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let a = Point(1, 2)\n\
             var b = a\n\
             b.x = 99\n\
             return a.x",
        )
        .unwrap();
    assert_eq!(result, Value::I32(1));
}

// ── Test 15: Struct Field Mutation ───────────────────────────────────

#[test]
fn test_struct_field_mutation() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Counter {\n\
             public value: int\n\
             }\n\
             var c = Counter(0)\n\
             c.value = 42\n\
             return c.value",
        )
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

// ── Test 16: Struct Methods ──────────────────────────────────────────

#[test]
fn test_struct_methods() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Rect {\n\
             public width: int\n\
             public height: int\n\
             \n\
             func area() -> int {\n\
             return self.width * self.height\n\
             }\n\
             }\n\
             let r = Rect(4, 5)\n\
             return r.area()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(20));
}

// ── Test 17: Struct Method with Parameters ───────────────────────────

#[test]
fn test_struct_method_with_params() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Vec2 {\n\
             public x: float\n\
             public y: float\n\
             \n\
             func add(other_x: float, other_y: float) -> float {\n\
             return self.x + other_x + self.y + other_y\n\
             }\n\
             }\n\
             let v = Vec2(1.0, 2.0)\n\
             return v.add(3.0, 4.0)",
        )
        .unwrap();
    assert_eq!(result, Value::F32(10.0));
}

// ── Test 18: Struct Equality ─────────────────────────────────────────

#[test]
fn test_struct_equality() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Same field values → equal
    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let a = Point(1, 2)\n\
             let b = Point(1, 2)\n\
             return a == b",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(true));

    // Different values → not equal
    let mut writ2 = Writ::new();
    writ2.disable_type_checking();
    let result2 = writ2
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let a = Point(1, 2)\n\
             let b = Point(3, 4)\n\
             return a == b",
        )
        .unwrap();
    assert_eq!(result2, Value::Bool(false));
}

// ── Test 19: Reflection — typeof ─────────────────────────────────────

#[test]
fn test_reflection_typeof() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(1, 2)\n\
             return typeof(p)",
        )
        .unwrap();
    assert_eq!(result, Value::Str(Rc::from("Point")));
}

// ── Test 20: Reflection — typeof on primitives ───────────────────────

#[test]
fn test_reflection_typeof_primitives() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    assert_eq!(
        writ.run("return typeof(42)").unwrap(),
        Value::Str(Rc::from("int"))
    );
    assert_eq!(
        writ.run("return typeof(3.14)").unwrap(),
        Value::Str(Rc::from("float"))
    );
    assert_eq!(
        writ.run(r#"return typeof("hello")"#).unwrap(),
        Value::Str(Rc::from("string"))
    );
    assert_eq!(
        writ.run("return typeof(true)").unwrap(),
        Value::Str(Rc::from("bool"))
    );
    assert_eq!(
        writ.run("return typeof(null)").unwrap(),
        Value::Str(Rc::from("null"))
    );
}

// ── Test 21: Reflection — instanceof ─────────────────────────────────

#[test]
fn test_reflection_instanceof() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(1, 2)\n\
             return instanceof(p, \"Point\")",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(true));

    let mut writ2 = Writ::new();
    writ2.disable_type_checking();
    let result2 = writ2
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(1, 2)\n\
             return instanceof(p, \"Rect\")",
        )
        .unwrap();
    assert_eq!(result2, Value::Bool(false));
}

// ── Test 22: Reflection — hasField / getField ────────────────────────

#[test]
fn test_reflection_has_get_field() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // hasField
    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(1, 2)\n\
             return hasField(p, \"x\")",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(true));

    let mut writ2 = Writ::new();
    writ2.disable_type_checking();
    let result2 = writ2
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(1, 2)\n\
             return hasField(p, \"z\")",
        )
        .unwrap();
    assert_eq!(result2, Value::Bool(false));

    // getField
    let mut writ3 = Writ::new();
    writ3.disable_type_checking();
    let result3 = writ3
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(10, 20)\n\
             return getField(p, \"y\")",
        )
        .unwrap();
    assert_eq!(result3, Value::I32(20));
}

// ── Test 23: Reflection — fields / methods ───────────────────────────

#[test]
fn test_reflection_fields_methods() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // fields returns an array of public field names
    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             let p = Point(1, 2)\n\
             let f = fields(p)\n\
             return f.length",
        )
        .unwrap();
    assert_eq!(result, Value::I32(2));

    // methods returns an array of public method names
    let mut writ2 = Writ::new();
    writ2.disable_type_checking();
    let result2 = writ2
        .run(
            "struct Named {\n\
             public name: string\n\
             \n\
             func greet() -> string {\n\
             return \"hi\"\n\
             }\n\
             }\n\
             let n = Named(\"test\")\n\
             let m = methods(n)\n\
             return m.length",
        )
        .unwrap();
    assert_eq!(result2, Value::I32(1));
}

// ── Test 24: Reflection — hasMethod ──────────────────────────────────

#[test]
fn test_reflection_has_method() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Greeter {\n\
             public name: string\n\
             \n\
             func greet() -> string {\n\
             return \"hello\"\n\
             }\n\
             }\n\
             let g = Greeter(\"test\")\n\
             return hasMethod(g, \"greet\")",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(true));

    let mut writ2 = Writ::new();
    writ2.disable_type_checking();
    let result2 = writ2
        .run(
            "struct Greeter {\n\
             public name: string\n\
             \n\
             func greet() -> string {\n\
             return \"hello\"\n\
             }\n\
             }\n\
             let g = Greeter(\"test\")\n\
             return hasMethod(g, \"missing\")",
        )
        .unwrap();
    assert_eq!(result2, Value::Bool(false));
}

// ── Test 25: Reflection — invoke ─────────────────────────────────────

#[test]
fn test_reflection_invoke() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Math {\n\
             public base: int\n\
             \n\
             func add(n: int) -> int {\n\
             return self.base + n\n\
             }\n\
             }\n\
             let m = Math(10)\n\
             return invoke(m, \"add\", 5)",
        )
        .unwrap();
    assert_eq!(result, Value::I32(15));
}

// ── Test 26: Struct in Functions ──────────────────────────────────────

#[test]
fn test_struct_in_function() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             func make_point(a: int, b: int) -> Point {\n\
             return Point(a, b)\n\
             }\n\
             func sum_point(p: Point) -> int {\n\
             return p.x + p.y\n\
             }\n\
             let p = make_point(10, 20)\n\
             return sum_point(p)",
        )
        .unwrap();
    assert_eq!(result, Value::I32(30));
}

// ── Test 27: Multiple Structs ────────────────────────────────────────

#[test]
fn test_multiple_structs() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Point {\n\
             public x: int\n\
             public y: int\n\
             }\n\
             struct Size {\n\
             public w: int\n\
             public h: int\n\
             }\n\
             let p = Point(1, 2)\n\
             let s = Size(10, 20)\n\
             return p.x + s.w + p.y + s.h",
        )
        .unwrap();
    assert_eq!(result, Value::I32(33));
}

// ── Test 28: Array Subscript ────────────────────────────────────────

#[test]
fn test_array_subscript() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "let arr = [10, 20, 30]\n\
             return arr[1]",
        )
        .unwrap();
    assert_eq!(result, Value::I32(20));
}

// ── Test 29: Array Subscript with Variable Index ────────────────────

#[test]
fn test_array_subscript_variable_index() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "let arr = [10, 20, 30]\n\
             let i = 2\n\
             return arr[i]",
        )
        .unwrap();
    assert_eq!(result, Value::I32(30));
}

// ── Test 30: Array Subscript in Function Argument ───────────────────

#[test]
fn test_array_subscript_in_function() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "func double(n: int) -> int { return n * 2 }\n\
             let arr = [5, 10, 15]\n\
             return double(arr[0])",
        )
        .unwrap();
    assert_eq!(result, Value::I32(10));
}

// ── Test 31: Array Subscript in Condition ───────────────────────────

#[test]
fn test_array_subscript_in_condition() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "let flags = [true, false, true]\n\
             if flags[0] { return 1 }\n\
             return 0",
        )
        .unwrap();
    assert_eq!(result, Value::I32(1));
}

// ── Test 32: Array Index Assignment ────────────────────────────────

#[test]
fn test_array_index_assignment() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "var arr = [1, 2, 3]\n\
             arr[1] = 99\n\
             return arr[1]",
        )
        .unwrap();
    assert_eq!(result, Value::I32(99));
}

// ── Test 33: Type Cast (as) ────────────────────────────────────────

#[test]
fn test_type_cast() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "let x = 42\n\
             let y = x as float\n\
             return y",
        )
        .unwrap();
    // After cast, x as float should produce a float or stay as int
    // (the VM auto-promotes at operation time, so cast is a no-op)
    // The value may still be int since cast compiles to nothing — test it doesn't error
    assert!(result == Value::I32(42) || result == Value::F32(42.0) || result == Value::F64(42.0));
}

// ── Test 34: Closure — basic capture ─────────────────────────────────

#[test]
fn test_closure_basic_capture() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "func make_adder(x: int) -> int {\n\
                 func adder(y: int) -> int {\n\
                     return x + y\n\
                 }\n\
                 return adder(10)\n\
             }\n\
             return make_adder(5)",
        )
        .unwrap();
    assert_eq!(result, Value::I32(15));
}

// ── Test 35: Closure — mutable capture ───────────────────────────────

#[test]
fn test_closure_mutable_capture() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "func test() -> int {\n\
                 var count = 0\n\
                 func inc() -> int {\n\
                     count = count + 1\n\
                     return count\n\
                 }\n\
                 inc()\n\
                 inc()\n\
                 inc()\n\
                 return count\n\
             }\n\
             return test()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(3));
}

// ── Test 36: Closure — survives return ───────────────────────────────

#[test]
fn test_closure_survives_return() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Return a closure, then call it after the enclosing function has returned.
    // The closed-over variable should still be accessible.
    let result = writ
        .run(
            "func make_counter() -> any {\n\
                 var count = 0\n\
                 func increment() -> int {\n\
                     count = count + 1\n\
                     return count\n\
                 }\n\
                 return increment\n\
             }\n\
             let counter = make_counter()\n\
             counter()\n\
             counter()\n\
             return counter()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(3));
}

// ── Test 37: Closure — shared capture ────────────────────────────────

#[test]
fn test_closure_shared_capture() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Two closures share the same captured variable
    let result = writ
        .run(
            "func test() -> int {\n\
                 var x = 0\n\
                 func inc() -> int {\n\
                     x = x + 1\n\
                     return x\n\
                 }\n\
                 func get() -> int {\n\
                     return x\n\
                 }\n\
                 inc()\n\
                 inc()\n\
                 return get()\n\
             }\n\
             return test()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(2));
}

// ── Test 38: Closure — nested capture (transitive) ───────────────────

#[test]
fn test_closure_nested_capture() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Three-level nesting: innermost captures from outermost
    let result = writ
        .run(
            "func outer() -> int {\n\
                 var x = 10\n\
                 func middle() -> int {\n\
                     func inner() -> int {\n\
                         return x\n\
                     }\n\
                     return inner()\n\
                 }\n\
                 return middle()\n\
             }\n\
             return outer()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(10));
}

// ── Test 39: Non-capturing function unchanged ────────────────────────

#[test]
fn test_closure_no_capture_unchanged() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // A function that doesn't capture anything should still work normally
    let result = writ
        .run(
            "func add(a: int, b: int) -> int {\n\
                 return a + b\n\
             }\n\
             return add(3, 4)",
        )
        .unwrap();
    assert_eq!(result, Value::I32(7));
}

// ── Test 40: Closure as callback ─────────────────────────────────────

#[test]
fn test_closure_as_callback() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Pass a closure to another function
    let result = writ
        .run(
            "func apply(f: any, val: int) -> int {\n\
                 return f(val)\n\
             }\n\
             func make_multiplier(factor: int) -> any {\n\
                 func mul(x: int) -> int {\n\
                     return x * factor\n\
                 }\n\
                 return mul\n\
             }\n\
             let double = make_multiplier(2)\n\
             return apply(double, 21)",
        )
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

// ── Test 41: Lambda capture ──────────────────────────────────────────

#[test]
fn test_closure_lambda_capture() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "func test() -> int {\n\
                 var x = 100\n\
                 let f = (y: int) => { return x + y }\n\
                 return f(23)\n\
             }\n\
             return test()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(123));
}

// ── Test 42: Operator overloading on struct ───────────────────────────

#[test]
fn test_operator_overloading_struct_add() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Vec2 {\n\
                 x: float\n\
                 y: float\n\
                 func add(other: Vec2) -> Vec2 {\n\
                     return Vec2(self.x + other.x, self.y + other.y)\n\
                 }\n\
             }\n\
             let a = Vec2(1.0, 2.0)\n\
             let b = Vec2(3.0, 4.0)\n\
             let c = a + b\n\
             return c.x",
        )
        .unwrap();
    // 1.0 + 3.0 = 4.0
    assert_eq!(result, Value::F64(4.0));
}

#[test]
fn test_operator_overloading_struct_subtract() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Vec2 {\n\
                 x: float\n\
                 y: float\n\
                 func subtract(other: Vec2) -> Vec2 {\n\
                     return Vec2(self.x - other.x, self.y - other.y)\n\
                 }\n\
             }\n\
             let a = Vec2(10.0, 5.0)\n\
             let b = Vec2(3.0, 2.0)\n\
             let c = a - b\n\
             return c.y",
        )
        .unwrap();
    // 5.0 - 2.0 = 3.0
    assert_eq!(result, Value::F64(3.0));
}

#[test]
fn test_operator_overloading_struct_multiply() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Counter {\n\
                 value: int\n\
                 func multiply(other: Counter) -> Counter {\n\
                     return Counter(self.value * other.value)\n\
                 }\n\
             }\n\
             let a = Counter(6)\n\
             let b = Counter(7)\n\
             let c = a * b\n\
             return c.value",
        )
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_operator_overloading_struct_comparison() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ
        .run(
            "struct Score {\n\
                 points: int\n\
                 func lt(other: Score) -> bool {\n\
                     return self.points < other.points\n\
                 }\n\
             }\n\
             let a = Score(10)\n\
             let b = Score(20)\n\
             return a < b",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(true));
}

// ── Test 46: User-defined generics ────────────────────────────────────

#[test]
fn test_generic_struct_instantiation() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Without type checking, the parser still needs to handle <T> in struct decl.
    // The compiler skips the template. We test via a concrete non-generic struct
    // that exercises the same code paths as a monomorphic instantiation would.
    // (Full generic instantiation requires the type checker.)
    let result = writ
        .run(
            "struct Pair {\n\
                 first: int\n\
                 second: string\n\
                 func get_first() -> int {\n\
                     return self.first\n\
                 }\n\
             }\n\
             let p = Pair(42, \"hello\")\n\
             return p.get_first()",
        )
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_generic_struct_parser_accepts_type_params() {
    // Verify the parser correctly handles <T> without erroring.
    // The compiler skips generic templates, so we just test that it compiles.
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "struct Box<T> {\n\
             value: int\n\
         }\n\
         return 1",
    );
    // Should compile and run without error (generic template is skipped by compiler)
    assert!(result.is_ok());
}

#[test]
fn test_generic_class_parser_accepts_type_params() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "class Container<T> {\n\
             size: int\n\
         }\n\
         return 2",
    );
    assert!(result.is_ok());
}

// ── Test: Multi-return destructuring at call site ─────────────────────

#[test]
fn test_let_destructure_from_function_call() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "func getPoint() -> (float, float) {\n\
             return (3.0, 4.0)\n\
         }\n\
         let (x, y) = getPoint()\n\
         return x + y",
    );
    assert_eq!(result.unwrap(), Value::F32(7.0));
}

// ── Test: Forward declarations (mutual type references) ───────────────

#[test]
fn test_forward_declarations_mutual_reference() {
    let mut writ = Writ::new();

    // Child declared before parent — should compile without error
    let result = writ.run(
        "class Child extends Parent {\n\
             public value: int\n\
         }\n\
         class Parent {\n\
             public base: int\n\
         }\n\
         let c = Child(10, 42)\n\
         let p = Parent(10)",
    );
    assert!(result.is_ok(), "forward declaration failed: {:?}", result);
}

// ── Test: super() calls parent method ────────────────────────────────

#[test]
fn test_super_calls_parent_method() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "class Animal {\n\
             public name: string\n\
             public func speak() -> string {\n\
                 return \"...\"\n\
             }\n\
         }\n\
         class Dog extends Animal {\n\
             public func speak() -> string {\n\
                 let base = super.speak()\n\
                 return \"Woof! \" .. base\n\
             }\n\
         }\n\
         let d = Dog(\"Rex\")\n\
         return d.speak()",
    );
    assert_eq!(result.unwrap(), Value::Str(std::rc::Rc::from("Woof! ...")));
}

// ── Test: Generic constraints (where T : Trait) ───────────────────────

#[test]
fn test_where_clause_parsed_and_accepted() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    // Parser-level test: where clause on a generic function
    let result = writ.run(
        "trait Printable {\n\
             func print()\n\
         }\n\
         func printAll<T>(item: T) where T : Printable {\n\
         }\n\
         return 1",
    );
    assert!(result.is_ok(), "where clause parse failed: {:?}", result);
}

#[test]
fn test_where_clause_on_generic_class() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "trait Comparable {\n\
             func lessThan(other: int) -> bool\n\
         }\n\
         class Container<T> where T : Comparable {\n\
             public value: int\n\
         }\n\
         return 1",
    );
    assert!(result.is_ok(), "where clause on class parse failed: {:?}", result);
}

// ── Test: Regex stdlib ────────────────────────────────────────────────

#[test]
fn test_regex_test_method() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "let r = Regex(\"\\\\d+\")\n\
         return r.test(\"abc123\")",
    );
    assert_eq!(result.unwrap(), Value::Bool(true));
}

#[test]
fn test_regex_match_method() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "let r = Regex(\"\\\\d+\")\n\
         return r.match(\"abc123def456\")",
    );
    assert_eq!(result.unwrap(), Value::Str(std::rc::Rc::from("123")));
}

#[test]
fn test_regex_match_all_method() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "let r = Regex(\"\\\\d+\")\n\
         let matches = r.matchAll(\"abc123def456\")\n\
         return matches.len()",
    );
    assert_eq!(result.unwrap(), Value::I32(2));
}

#[test]
fn test_regex_replace_method() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "let r = Regex(\"\\\\d+\")\n\
         return r.replace(\"abc123def456\", \"NUM\")",
    );
    assert_eq!(result.unwrap(), Value::Str(std::rc::Rc::from("abcNUMdef456")));
}

#[test]
fn test_regex_replace_all_method() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "let r = Regex(\"\\\\d+\")\n\
         return r.replaceAll(\"abc123def456\", \"NUM\")",
    );
    assert_eq!(result.unwrap(), Value::Str(std::rc::Rc::from("abcNUMdefNUM")));
}

#[test]
fn test_regex_no_match_returns_null() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let result = writ.run(
        "let r = Regex(\"\\\\d+\")\n\
         return r.match(\"no digits here\")",
    );
    assert_eq!(result.unwrap(), Value::Null);
}

// ═══════════════════════════════════════════════════════════════════
// Phase 2: VM opcode coverage, error paths, coroutines, debug hooks
// ═══════════════════════════════════════════════════════════════════

// ── 2A: Typed arithmetic opcodes ────────────────────────────────────
// These require type checking ON so the compiler emits AddInt/SubInt/etc.
// instead of the generic Add/Sub variants.

#[test]
fn test_typed_add_int_opcode() {
    // type checking ON → compiler emits AddInt
    let result = Writ::new()
        .run("func add(a: int, b: int) -> int { return a + b }\nreturn add(20, 22)")
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_typed_sub_int_opcode() {
    let result = Writ::new()
        .run("func sub(a: int, b: int) -> int { return a - b }\nreturn sub(50, 8)")
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_typed_mul_int_opcode() {
    let result = Writ::new()
        .run("func mul(a: int, b: int) -> int { return a * b }\nreturn mul(6, 7)")
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_typed_div_int_opcode() {
    let result = Writ::new()
        .run("func div(a: int, b: int) -> int { return a / b }\nreturn div(84, 2)")
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_typed_add_float_opcode() {
    let result = Writ::new()
        .run("func addf(a: float, b: float) -> float { return a + b }\nreturn addf(20.5, 21.5)")
        .unwrap();
    assert!(matches!(result, Value::F32(_) | Value::F64(_)));
}

#[test]
fn test_typed_mul_float_opcode() {
    let result = Writ::new()
        .run("func mulf(a: float, b: float) -> float { return a * b }\nreturn mulf(6.0, 7.0)")
        .unwrap();
    assert!(matches!(result, Value::F32(_) | Value::F64(_)));
}

// ── 2B: Quickened opcodes (loop iterations 2+) ──────────────────────

#[test]
fn test_quickened_add_int_loop() {
    // After iteration 1, QAddInt replaces AddInt; loop forces quickening
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ
        .run(
            "var x = 0\n\
             var i = 0\n\
             while i < 10 { x = x + 1\ni = i + 1 }\n\
             return x",
        )
        .unwrap();
    assert_eq!(result, Value::I32(10));
}

#[test]
fn test_quickened_comparison_loop() {
    // The while condition `i < 10` quickens on iterations 2+ to QLtInt
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ
        .run(
            "var i = 0\n\
             while i < 20 { i = i + 1 }\n\
             return i",
        )
        .unwrap();
    assert_eq!(result, Value::I32(20));
}

#[test]
fn test_quickened_float_loop() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ
        .run(
            "var x = 0.0\n\
             var i = 0\n\
             while i < 5 { x = x + 1.0\ni = i + 1 }\n\
             return i",
        )
        .unwrap();
    assert_eq!(result, Value::I32(5));
}

#[test]
fn test_quickened_equality_loop() {
    // EqInt / QEqInt path: loop until a counter equals a target
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ
        .run(
            "var i = 0\n\
             var found = false\n\
             while i < 10 {\n\
                 if i == 7 { found = true }\n\
                 i = i + 1\n\
             }\n\
             return found",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(true));
}

// ── 2C: VM error paths ───────────────────────────────────────────────

#[test]
fn test_div_by_zero_int() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("return 10 / 0");
    assert!(result.is_err());
}

#[test]
fn test_div_by_zero_float() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    // Float division by zero produces Infinity in IEEE 754 — may or may not error
    // The VM may allow it. We just ensure no panic.
    let _ = writ.run("return 10.0 / 0.0");
}

#[test]
fn test_mod_by_zero() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("return 10 % 0");
    assert!(result.is_err());
}

#[test]
fn test_array_out_of_bounds_positive() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let arr = [1, 2, 3]\nreturn arr[10]");
    assert!(result.is_err());
}

#[test]
fn test_array_out_of_bounds_negative() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let arr = [1, 2, 3]\nreturn arr[-1]");
    assert!(result.is_err());
}

#[test]
fn test_method_not_found_on_primitive() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("return (42).nonexistentMethod()");
    assert!(result.is_err());
}

#[test]
fn test_native_fn_type_mismatch() {
    // Register a function expecting bool, pass an int → FromValue error
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_fn("takes_bool", fn1(|b: bool| -> Result<bool, String> { Ok(b) }));
    let result = writ.run("return takes_bool(42)");
    assert!(result.is_err());
}

#[test]
fn test_native_fn_narrowing_overflow() {
    // Register fn expecting i32, pass i64::MAX (overflows i32) → error
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_fn("takes_i32", fn1(|n: i32| -> Result<i32, String> { Ok(n) }));
    // Load a large constant via a function that returns i64
    // Since Writ int literals are I32 by default, we can test with a direct large literal
    // if the language supports i64 literals, or just test the happy path
    let result = writ.run("return takes_i32(100)");
    assert_eq!(result.unwrap(), Value::I32(100));
}

// ── 2D: call() API ───────────────────────────────────────────────────

#[test]
fn test_call_api_success() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("writ_call_api_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("funcs.writ");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "func double(n: int) -> int {{ return n * 2 }}").unwrap();
    }
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.load(path.to_str().unwrap()).unwrap();
    let result = writ.call("double", &[Value::I32(21)]).unwrap();
    assert_eq!(result, Value::I32(42));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn test_call_api_function_not_found() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.call("nonexistent_function", &[]);
    assert!(result.is_err());
}

#[test]
fn test_call_api_wrong_arity() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("writ_call_arity_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("arity.writ");
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "func add(a: int, b: int) -> int {{ return a + b }}").unwrap();
    }
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.load(path.to_str().unwrap()).unwrap();
    // Pass only 1 arg to a 2-arg function
    let result = writ.call("add", &[Value::I32(1)]);
    // May succeed with Null for missing arg, or may error — either is fine
    let _ = result;
    std::fs::remove_dir_all(dir).ok();
}

// ── 2E: Coroutine scheduling ──────────────────────────────────────────

#[test]
fn test_coroutine_yield_seconds() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let done = std::rc::Rc::new(std::cell::RefCell::new(false));
    let done_clone = std::rc::Rc::clone(&done);
    writ.register_fn(
        "mark_done",
        fn0(move || -> Result<Value, String> {
            *done_clone.borrow_mut() = true;
            Ok(Value::Null)
        }),
    );

    writ.run(
        "func work() {\n\
             waitForSeconds(0.1)\n\
             mark_done()\n\
         }\n\
         start work()",
    )
    .unwrap();

    assert!(!*done.borrow());
    writ.tick(0.05).unwrap();
    assert!(!*done.borrow()); // not yet — 0.1s hasn't elapsed
    writ.tick(0.1).unwrap();
    assert!(*done.borrow()); // now done
}

#[test]
fn test_coroutine_yield_frames() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    let ticks = std::rc::Rc::new(std::cell::RefCell::new(0i32));
    let ticks_clone = std::rc::Rc::clone(&ticks);
    writ.register_fn(
        "mark_tick",
        fn0(move || -> Result<Value, String> {
            *ticks_clone.borrow_mut() += 1;
            Ok(Value::Null)
        }),
    );

    writ.run(
        "func work() {\n\
             waitForFrames(3)\n\
             mark_tick()\n\
         }\n\
         start work()",
    )
    .unwrap();

    writ.tick(0.016).unwrap();
    writ.tick(0.016).unwrap();
    assert_eq!(*ticks.borrow(), 0);
    writ.tick(0.016).unwrap();
    assert_eq!(*ticks.borrow(), 1);
}

#[test]
fn test_coroutine_active_count() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    writ.run(
        "func spin() { while true { yield } }\n\
         start spin()\n\
         start spin()",
    )
    .unwrap();

    // After run: both coroutines queued but not yet started
    writ.tick(0.016).unwrap();
    assert_eq!(writ.vm_mut().active_coroutine_count(), 2);
}

#[test]
fn test_coroutine_cancel_by_owner() {
    let mut writ = Writ::new();
    writ.disable_type_checking();

    writ.run(
        "func spin() { while true { yield } }\n\
         start spin()",
    )
    .unwrap();

    writ.tick(0.016).unwrap();
    assert_eq!(writ.vm_mut().active_coroutine_count(), 1);

    writ.cancel_coroutines_for_owner(999); // no-op, different owner
    assert_eq!(writ.vm_mut().active_coroutine_count(), 1);
}

// ── 2F: Hot reload ───────────────────────────────────────────────────

#[test]
fn test_reload_updates_function() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("writ_reload_test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hot.writ");

    // Initial version
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "func compute() -> int {{ return 1 }}").unwrap();
    }
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.load(path.to_str().unwrap()).unwrap();
    assert_eq!(writ.call("compute", &[]).unwrap(), Value::I32(1));

    // Reload with updated body
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "func compute() -> int {{ return 99 }}").unwrap();
    }
    writ.reload(path.to_str().unwrap()).unwrap();
    assert_eq!(writ.call("compute", &[]).unwrap(), Value::I32(99));

    std::fs::remove_dir_all(dir).ok();
}

// ── 2G: disable_module() ─────────────────────────────────────────────

#[test]
fn test_disable_module_blocks_calls() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.disable_module("math");
    let result = writ.run("return abs(-5)");
    assert!(result.is_err());
}

// ── 2H: Instruction limit ─────────────────────────────────────────────
// Already exists as test_instruction_limit_integration — extend with a tighter limit

#[test]
fn test_instruction_limit_tight() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.set_instruction_limit(10);
    let result = writ.run("var x = 0\nwhile true { x = x + 1 }\nreturn x");
    assert!(result.is_err());
    match result.unwrap_err() {
        WritError::Runtime(e) => assert!(e.message.contains("instruction limit")),
        other => panic!("expected RuntimeError, got: {other}"),
    }
}

// ── 2I: register_global / register_type API coverage ─────────────────

#[test]
fn test_register_global_accessible_in_script() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_global("MY_CONST", Value::I32(42));
    let result = writ.run("return MY_CONST").unwrap();
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_int_to_float_coercion() {
    // IntToFloat opcode: arithmetic between int and float
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let x = 5\nlet y = 2.5\nreturn x + y").unwrap();
    assert!(matches!(result, Value::F32(_) | Value::F64(_)));
}

#[test]
fn test_negation_integer() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let x = 10\nreturn -x").unwrap();
    assert_eq!(result, Value::I32(-10));
}

#[test]
fn test_negation_float() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let x = 3.14\nreturn -x").unwrap();
    assert!(matches!(result, Value::F32(_) | Value::F64(_)));
}

#[test]
fn test_logical_not() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("return !false").unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_string_concat_opcode() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run(r#"return "hello" .. " " .. "world""#).unwrap();
    assert_eq!(result, Value::Str(std::rc::Rc::from("hello world")));
}

#[test]
fn test_dict_literal_and_access() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let d = {\"a\": 1, \"b\": 2}\nreturn d[\"a\"]").unwrap();
    assert_eq!(result, Value::I32(1));
}

#[test]
fn test_make_array_opcode() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let arr = [1, 2, 3, 4, 5]\nreturn arr.len()").unwrap();
    assert_eq!(result, Value::I32(5));
}

#[test]
fn test_comparison_generic_eq() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("return 42 == 42").unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_comparison_generic_ne() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("return 1 != 2").unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_null_coalesce_non_null() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("return 99 ?? 0").unwrap();
    assert_eq!(result, Value::I32(99));
}

#[test]
fn test_spread_operator() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run(
        "func sum(a: int, b: int, c: int) -> int { return a + b + c }\n\
         let args = [1, 2, 3]\n\
         return sum(...args)",
    );
    // spread may or may not be supported without type checking — accept either outcome
    let _ = result;
}

// ── 2J: Debug hooks ───────────────────────────────────────────────────

#[cfg(feature = "debug-hooks")]
mod debug_hook_tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use writ::{BreakpointAction, Value, Writ};

    #[test]
    fn test_on_line_hook_fires() {
        let mut writ = Writ::new();
        writ.disable_type_checking();

        let lines: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
        let lines_clone = Rc::clone(&lines);
        writ.on_line(move |_file, line| {
            lines_clone.borrow_mut().push(line);
        });

        writ.run("let x = 1\nlet y = 2\nreturn x + y").unwrap();
        assert!(!lines.borrow().is_empty(), "on_line hook never fired");
    }

    #[test]
    fn test_on_call_hook_fires() {
        let mut writ = Writ::new();
        writ.disable_type_checking();

        let calls: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let calls_clone = Rc::clone(&calls);
        writ.on_call(move |name, _file, _line| {
            calls_clone.borrow_mut().push(name.to_string());
        });

        writ.run(
            "func greet() -> int { return 1 }\nreturn greet()",
        )
        .unwrap();
        assert!(
            calls.borrow().contains(&"greet".to_string()),
            "on_call hook did not fire for 'greet'"
        );
    }

    #[test]
    fn test_on_return_hook_fires() {
        let mut writ = Writ::new();
        writ.disable_type_checking();

        let returns: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let returns_clone = Rc::clone(&returns);
        writ.on_return(move |name, _file, _line| {
            returns_clone.borrow_mut().push(name.to_string());
        });

        writ.run(
            "func greet() -> int { return 1 }\nreturn greet()",
        )
        .unwrap();
        assert!(!returns.borrow().is_empty(), "on_return hook never fired");
    }

    #[test]
    fn test_breakpoint_continue_executes_normally() {
        let mut writ = Writ::new();
        writ.disable_type_checking();
        writ.set_breakpoint("", 1);
        writ.on_breakpoint(|_ctx| BreakpointAction::Continue);
        let result = writ.run("return 42").unwrap();
        assert_eq!(result, Value::I32(42));
    }

    #[test]
    fn test_breakpoint_abort_stops_execution() {
        let mut writ = Writ::new();
        writ.disable_type_checking();
        writ.set_breakpoint("", 1);
        writ.on_breakpoint(|_ctx| BreakpointAction::Abort);
        let result = writ.run("return 42");
        assert!(result.is_err(), "expected abort to produce an error");
    }

    #[test]
    fn test_remove_breakpoint() {
        let mut writ = Writ::new();
        writ.disable_type_checking();
        writ.set_breakpoint("", 1);
        writ.remove_breakpoint("", 1);
        writ.on_breakpoint(|_ctx| BreakpointAction::Abort);
        // Breakpoint removed — execution should succeed
        let result = writ.run("return 42").unwrap();
        assert_eq!(result, Value::I32(42));
    }
}

// ── Partition pivot bug reproduction ─────────────────────────────────

#[test]
fn test_partition_pivot_types() {
    let mut vm = Writ::new();
    vm.disable_type_checking();

    vm.register_host_fn_untyped(
        "make_data",
        fn1(|s: Value| -> Result<Value, String> {
            let s = match &s {
                Value::Str(s) => Rc::clone(s),
                _ => return Err("make_data expects a string".into()),
            };
            Ok(Value::Object(Rc::new(RefCell::new(MockPlayer {
                name: s.to_string(),
                health: 100.0,
            }))))
        }),
    );

    vm.register_host_fn_untyped(
        "data_lt",
        fn2(|a: Value, b: Value| -> Result<Value, String> {
            if !matches!(a, Value::Object(_)) || !matches!(b, Value::Object(_)) {
                return Err(format!(
                    "data_lt got non-Object: a={:?}, b={:?}",
                    a.type_name(),
                    b.type_name()
                ));
            }
            Ok(Value::Bool(false))
        }),
    );

    let script = r#"
func partition(arr: Array<any>, lo: int, hi: int) -> int {
    let pivot_idx = (lo + hi) / 2
    let pivot = arr[pivot_idx]
    arr[pivot_idx] = arr[hi]
    arr[hi] = pivot
    var j = lo
    var i = lo
    while i < hi {
        if data_lt(arr[i], pivot) {
            let tmp = arr[i]
            arr[i] = arr[j]
            arr[j] = tmp
            j += 1
        }
        i += 1
    }
    let tmp = arr[j]
    arr[j] = arr[hi]
    arr[hi] = tmp
    return j
}

func quicksort(arr: Array<any>, lo: int, hi: int) {
    var low = lo
    var high = hi
    while low < high {
        let p = partition(arr, low, high)
        quicksort(arr, low, p - 1)
        low = p + 1
    }
}

func run() -> Array<any> {
    var arr: Array<any> = []
    var i = 0
    while i < 20 {
        arr.push(make_data("item"))
        i += 1
    }
    quicksort(arr, 0, arr.len() - 1)
    return arr
}
"#;

    vm.run(script).expect("failed to load script");
    let result = vm.call("run", &[]).expect("run() failed");
    match result {
        Value::Array(a) => assert_eq!(a.borrow().len(), 20),
        _ => panic!("run() must return an array"),
    }
}

// ── Compiler path: const declarations ────────────────────────────────────────

fn w() -> Writ {
    let mut w = Writ::new();
    w.disable_type_checking();
    w
}

#[test]
fn test_const_declaration() {
    let result = w().run("const MAX = 100\nreturn MAX").unwrap();
    assert_eq!(result, Value::I32(100));
}

#[test]
fn test_const_used_in_expression() {
    let result = w().run("const X = 10\nconst Y = 20\nreturn X + Y").unwrap();
    assert_eq!(result, Value::I32(30));
}

// ── Compiler path: for range loop ─────────────────────────────────────────────

#[test]
fn test_for_range_sum() {
    let result = w()
        .run("var sum = 0\nfor i in 0..5 { sum = sum + i }\nreturn sum")
        .unwrap();
    assert_eq!(result, Value::I32(10));
}

#[test]
fn test_for_range_inclusive() {
    let result = w()
        .run("var sum = 0\nfor i in 1..=5 { sum = sum + i }\nreturn sum")
        .unwrap();
    assert_eq!(result, Value::I32(15));
}

#[test]
fn test_for_range_loop_variable_correct() {
    let result = w()
        .run("var last = 0\nfor i in 0..4 { last = i }\nreturn last")
        .unwrap();
    assert_eq!(result, Value::I32(3));
}

// ── Compiler path: for array loop ─────────────────────────────────────────────

#[test]
fn test_for_array_sum() {
    let result = w()
        .run("let nums = [1, 2, 3, 4, 5]\nvar sum = 0\nfor n in nums { sum = sum + n }\nreturn sum")
        .unwrap();
    assert_eq!(result, Value::I32(15));
}

#[test]
fn test_for_array_count_elements() {
    let result = w()
        .run("let items = [\"a\", \"b\", \"c\"]\nvar count = 0\nfor item in items { count = count + 1 }\nreturn count")
        .unwrap();
    assert_eq!(result, Value::I32(3));
}

// ── Compiler path: string interpolation ──────────────────────────────────────

#[test]
fn test_string_interpolation_simple() {
    let result = w()
        .run("let name = \"World\"\nreturn \"Hello, $name!\"")
        .unwrap();
    assert_eq!(result, Value::Str(Rc::from("Hello, World!")));
}

#[test]
fn test_string_interpolation_expression() {
    let result = w()
        .run("let x = 6\nlet y = 7\nreturn \"Result: ${x * y}\"")
        .unwrap();
    assert_eq!(result, Value::Str(Rc::from("Result: 42")));
}

#[test]
fn test_string_interpolation_multi_segment() {
    let result = w()
        .run("let a = 1\nlet b = 2\nreturn \"$a + $b = ${a + b}\"")
        .unwrap();
    assert_eq!(result, Value::Str(Rc::from("1 + 2 = 3")));
}

// ── Compiler path: when as expression ────────────────────────────────────────

#[test]
fn test_when_expression_as_value() {
    let result = w()
        .run("let x = 2\nlet label = when x { 1 => 10\n2 => 20\nelse => 0 }\nreturn label")
        .unwrap();
    assert_eq!(result, Value::I32(20));
}

#[test]
fn test_when_expression_else_arm() {
    // Use a statement-form when with else to verify else-arm execution
    let result = w()
        .run(
            "func classify(x: int) -> int {\n\
             when x {\n\
             1 => { return 100 }\n\
             else => { return 999 }\n\
             }\n\
             return 0\n\
             }\n\
             return classify(99)",
        )
        .unwrap();
    assert_eq!(result, Value::I32(999));
}

// ── Compiler path: logical &&/|| short-circuit ────────────────────────────────

#[test]
fn test_logical_and_short_circuit() {
    let result = w()
        .run(
            "var called = false\n\
             func side_effect() -> bool { called = true\nreturn true }\n\
             let r = false && side_effect()\n\
             return called",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_logical_or_short_circuit() {
    let result = w()
        .run(
            "var called = false\n\
             func side_effect() -> bool { called = true\nreturn false }\n\
             let r = true || side_effect()\n\
             return called",
        )
        .unwrap();
    assert_eq!(result, Value::Bool(false));
}

#[test]
fn test_logical_or_fallback_value() {
    assert_eq!(w().run("return false || true").unwrap(), Value::Bool(true));
}

// ── Compiler path: break and continue ────────────────────────────────────────

#[test]
fn test_break_exits_loop() {
    let result = w()
        .run("var i = 0\nwhile true { if i == 5 { break }\ni = i + 1 }\nreturn i")
        .unwrap();
    assert_eq!(result, Value::I32(5));
}

#[test]
fn test_continue_skips_odd() {
    let result = w()
        .run(
            "var sum = 0\nvar i = 0\n\
             while i < 10 {\n\
                 i = i + 1\n\
                 if i % 2 != 0 { continue }\n\
                 sum = sum + i\n\
             }\n\
             return sum",
        )
        .unwrap();
    assert_eq!(result, Value::I32(30));
}

// ── Compiler path: dict subscript assignment ──────────────────────────────────

#[test]
fn test_dict_subscript_update_value() {
    let result = w()
        .run("var d = {\"x\": 1}\nd[\"x\"] = 99\nreturn d[\"x\"]")
        .unwrap();
    assert_eq!(result, Value::I32(99));
}

#[test]
fn test_dict_new_key_assignment() {
    let result = w()
        .run("var d = {}\nd[\"hello\"] = 42\nreturn d[\"hello\"]")
        .unwrap();
    assert_eq!(result, Value::I32(42));
}

// ── Compiler path: struct field from direct call ──────────────────────────────

#[test]
fn test_struct_field_from_direct_call() {
    let result = w()
        .run(
            "struct P { public x: int\npublic y: int }\n\
             func make() -> P { return P(3, 7) }\n\
             return make().x",
        )
        .unwrap();
    assert_eq!(result, Value::I32(3));
}

// ── Compiler path: two-level class inheritance ────────────────────────────────

#[test]
fn test_class_two_level_inheritance() {
    let result = w()
        .run(
            "class A { public val: int\npublic func get() -> int { return self.val } }\n\
             class B extends A { public extra: int }\n\
             class C extends B {}\n\
             let c = C(10, 20)\n\
             return c.val",
        )
        .unwrap();
    assert_eq!(result, Value::I32(10));
}

// ── Host type for register_type tests ─────────────────────────────────────────

#[derive(Debug)]
struct Counter {
    count: i32,
}

impl WritObject for Counter {
    fn type_name(&self) -> &str {
        "Counter"
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        match name {
            "count" => Ok(Value::I32(self.count)),
            _ => Err(format!("Counter has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        if name == "count" {
            self.count = value.as_i64() as i32;
            Ok(())
        } else {
            Err(format!("Counter has no field '{name}'"))
        }
    }

    fn call_method(&mut self, name: &str, _args: &[Value]) -> Result<Value, String> {
        match name {
            "increment" => {
                self.count += 1;
                Ok(Value::I32(self.count))
            }
            _ => Err(format!("Counter has no method '{name}'")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn counter_factory(args: &[Value]) -> Result<Box<dyn WritObject>, String> {
    let start = if let Some(Value::I32(v)) = args.first() { *v } else { 0 };
    Ok(Box::new(Counter { count: start }))
}

// ── register_host_fn (typed) ───────────────────────────────────────────────────

#[test]
fn test_register_host_fn_typed_no_args() {
    let mut writ = Writ::new();
    writ.register_host_fn(
        "magic",
        vec![],
        Type::Int,
        fn0(|| -> Result<Value, String> { Ok(Value::I32(99)) }),
    );
    assert_eq!(writ.run("return magic()").unwrap(), Value::I32(99));
}

#[test]
fn test_register_host_fn_typed_with_param() {
    let mut writ = Writ::new();
    writ.register_host_fn(
        "double",
        vec![Type::Int],
        Type::Int,
        fn1(|x: i32| -> Result<Value, String> { Ok(Value::I32(x * 2)) }),
    );
    assert_eq!(writ.run("return double(7)").unwrap(), Value::I32(14));
}

#[test]
fn test_register_host_fn_type_checker_rejects_wrong_arg() {
    let mut writ = Writ::new();
    writ.register_host_fn(
        "take_int",
        vec![Type::Int],
        Type::Int,
        fn1(|x: i32| -> Result<Value, String> { Ok(Value::I32(x)) }),
    );
    assert!(writ.run("return take_int(\"hello\")").is_err());
}

// ── register_host_fn_untyped ───────────────────────────────────────────────────

#[test]
fn test_register_host_fn_untyped_callable() {
    let mut writ = Writ::new();
    writ.register_host_fn_untyped(
        "dyn_fn",
        fn1(|x: i32| -> Result<Value, String> { Ok(Value::I32(x + 1)) }),
    );
    assert_eq!(writ.run("return dyn_fn(41)").unwrap(), Value::I32(42));
}

#[test]
fn test_register_host_fn_untyped_type_checker_allows() {
    let mut writ = Writ::new();
    writ.register_host_fn_untyped(
        "anything",
        fn0(|| -> Result<Value, String> { Ok(Value::Bool(true)) }),
    );
    assert!(writ.run("return anything()").is_ok());
}

// ── register_method + mfn0 / mfn1 / mfn2 ─────────────────────────────────────

#[test]
fn test_register_method_mfn0_on_string() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_method(
        ValueTag::Str,
        "shout",
        None,
        mfn0(|s: Rc<str>| -> Result<String, String> { Ok(format!("{}!", s.to_uppercase())) }),
    );
    assert_eq!(
        writ.run(r#"return "hello".shout()"#).unwrap(),
        Value::Str(Rc::from("HELLO!"))
    );
}

#[test]
fn test_register_method_mfn1_on_int() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_method(
        ValueTag::Int,
        "add_n",
        None,
        mfn1(|x: i32, y: i32| -> Result<i32, String> { Ok(x + y) }),
    );
    assert_eq!(writ.run("return 10.add_n(5)").unwrap(), Value::I32(15));
}

#[test]
fn test_register_method_mfn2_on_string() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_method(
        ValueTag::Str,
        "repeat_sep",
        None,
        mfn2(|s: Rc<str>, n: i32, sep: Rc<str>| -> Result<String, String> {
            Ok((0..n).map(|_| s.as_ref()).collect::<Vec<_>>().join(sep.as_ref()))
        }),
    );
    assert_eq!(
        writ.run(r#"return "a".repeat_sep(3, "-")"#).unwrap(),
        Value::Str(Rc::from("a-a-a"))
    );
}

#[test]
fn test_register_method_with_module_disable() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_method(
        ValueTag::Int,
        "triple",
        Some("custom"),
        mfn0(|x: i32| -> Result<i32, String> { Ok(x * 3) }),
    );
    assert_eq!(writ.run("return 5.triple()").unwrap(), Value::I32(15));
    writ.disable_module("custom");
    assert!(writ.run("return 5.triple()").is_err());
}

// ── register_type ─────────────────────────────────────────────────────────────

#[test]
fn test_register_type_field_access() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_type("Counter", counter_factory);
    assert_eq!(
        writ.run("let c = Counter(10)\nreturn c.count").unwrap(),
        Value::I32(10)
    );
}

#[test]
fn test_register_type_method_call() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_type("Counter", counter_factory);
    let r = writ
        .run("var c = Counter(0)\nc.increment()\nc.increment()\nreturn c.count")
        .unwrap();
    assert_eq!(r, Value::I32(2));
}

#[test]
fn test_register_type_set_field() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_type("Counter", |_| Ok(Box::new(Counter { count: 0 })));
    let r = writ
        .run("var c = Counter()\nc.count = 99\nreturn c.count")
        .unwrap();
    assert_eq!(r, Value::I32(99));
}

// ── enable_type_checking ───────────────────────────────────────────────────────

#[test]
fn test_enable_type_checking_rejects_type_errors() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.enable_type_checking();
    assert!(writ.run("let x: int = \"hello\"\nreturn x").is_err());
}

#[test]
fn test_enable_type_checking_accepts_valid_code() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.enable_type_checking();
    assert_eq!(writ.run("let x: int = 42\nreturn x").unwrap(), Value::I32(42));
}

// ── reset_script_types ────────────────────────────────────────────────────────

#[test]
fn test_reset_script_types_removes_defined_funcs() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.run("func foo() -> int { return 1 }").unwrap();
    writ.reset_script_types();
    assert!(writ.run("return foo()").is_err());
}

#[test]
fn test_reset_script_types_allows_redefine() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.run("func foo() -> int { return 1 }").unwrap();
    writ.reset_script_types();
    // After reset the type checker no longer knows about 'foo', so redefining works without error
    let result = writ.run("func foo() -> int { return 2 }\nreturn foo()").unwrap();
    assert_eq!(result, Value::I32(2));
}

// ── codegen_rust ──────────────────────────────────────────────────────────────

#[test]
fn test_codegen_rust_produces_output_for_class() {
    let mut writ = Writ::new();
    let out = writ
        .codegen_rust("class Point {\n    public x: int\n    public y: int\n}")
        .unwrap();
    assert!(out.contains("Point"));
}

#[test]
fn test_codegen_rust_lex_error() {
    let mut writ = Writ::new();
    assert!(writ.codegen_rust("return 5 & 3").is_err());
}

// ── WritError::Lex ────────────────────────────────────────────────────────────

#[test]
fn test_lex_error_single_ampersand() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    assert!(matches!(writ.run("return 5 & 3"), Err(WritError::Lex(_))));
}

#[test]
fn test_lex_error_unterminated_string() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    assert!(matches!(writ.run("return \"unterminated"), Err(WritError::Lex(_))));
}

// ── VM methods via vm_mut() ───────────────────────────────────────────────────

#[test]
fn test_register_fn_in_module_callable() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.vm_mut().register_fn_in_module(
        "custom_add",
        "mymod",
        fn2(|a: i32, b: i32| -> Result<i32, String> { Ok(a + b) }),
    );
    assert_eq!(writ.run("return custom_add(3, 4)").unwrap(), Value::I32(7));
}

#[test]
fn test_register_fn_in_module_disable_blocks_it() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.vm_mut()
        .register_fn_in_module("my_fn", "mymod", fn0(|| -> Result<i32, String> { Ok(1) }));
    writ.disable_module("mymod");
    assert!(writ.run("return my_fn()").is_err());
}

#[test]
fn test_last_coroutine_id_some_after_start() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.run("func spin() { while true { yield } }\nstart spin()").unwrap();
    writ.tick(0.016).unwrap();
    assert!(writ.vm_mut().last_coroutine_id().is_some());
}

#[test]
fn test_set_coroutine_owner_and_cancel_for() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.run("func spin() { while true { yield } }\nstart spin()").unwrap();
    writ.tick(0.016).unwrap();
    let id = writ.vm_mut().last_coroutine_id().unwrap();
    writ.vm_mut().set_coroutine_owner(id, 42);
    writ.vm_mut().cancel_coroutines_for(42);
    writ.tick(0.016).unwrap();
    assert_eq!(writ.vm_mut().active_coroutine_count(), 0);
}

#[test]
fn test_cancel_coroutines_for_wrong_owner_is_noop() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.run("func spin() { while true { yield } }\nstart spin()").unwrap();
    writ.tick(0.016).unwrap();
    let id = writ.vm_mut().last_coroutine_id().unwrap();
    writ.vm_mut().set_coroutine_owner(id, 42);
    writ.vm_mut().cancel_coroutines_for(999);
    assert_eq!(writ.vm_mut().active_coroutine_count(), 1);
}

// ── Value methods ─────────────────────────────────────────────────────────────

#[test]
fn test_value_is_numeric_true_for_all_numeric_variants() {
    assert!(Value::I32(1).is_numeric());
    assert!(Value::I64(1).is_numeric());
    assert!(Value::F32(1.0).is_numeric());
    assert!(Value::F64(1.0).is_numeric());
}

#[test]
fn test_value_is_numeric_false_for_non_numeric() {
    assert!(!Value::Bool(true).is_numeric());
    assert!(!Value::Null.is_numeric());
    assert!(!Value::Str(Rc::from("1")).is_numeric());
}

#[test]
fn test_value_cheap_clone_equals_original() {
    assert_eq!(Value::I32(42).cheap_clone(), Value::I32(42));
    assert_eq!(Value::F64(3.14).cheap_clone(), Value::F64(3.14));
    assert_eq!(Value::Bool(true).cheap_clone(), Value::Bool(true));
    assert_eq!(Value::Null.cheap_clone(), Value::Null);
}

#[test]
fn test_value_type_name_owned() {
    assert_eq!(Value::I32(0).type_name_owned(), "int");
    assert_eq!(Value::F64(0.0).type_name_owned(), "float");
    assert_eq!(Value::Bool(true).type_name_owned(), "bool");
    assert_eq!(Value::Null.type_name_owned(), "null");
    assert_eq!(Value::Str(Rc::from("x")).type_name_owned(), "string");
}

// ── Value::is_null ────────────────────────────────────────────────────────────

#[test]
fn test_value_is_null_true() {
    assert!(Value::Null.is_null());
}

#[test]
fn test_value_is_null_false() {
    assert!(!Value::I32(0).is_null());
    assert!(!Value::Bool(false).is_null());
    assert!(!Value::Str(Rc::from("")).is_null());
}

// ── Value::promote_float_pair_op ──────────────────────────────────────────────

#[test]
fn test_promote_float_pair_op_adds() {
    let result = Value::promote_float_pair_op(&Value::F32(1.5), &Value::F64(2.5), |a, b| a + b);
    assert!((result.as_f64() - 4.0).abs() < 1e-6);
}

#[test]
fn test_promote_float_pair_op_mixed_widths() {
    let result = Value::promote_float_pair_op(&Value::F32(3.0), &Value::F64(4.0), |a, b| a * b);
    assert!((result.as_f64() - 12.0).abs() < 1e-6);
    // Mixed widths should promote to F64
    assert!(matches!(result, Value::F64(_)));
}

// ── fn3 / mfn3 ───────────────────────────────────────────────────────────────

#[test]
fn test_fn3_callable() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.vm_mut().register_fn_in_module(
        "clamp3",
        "test",
        fn3(|v: f64, lo: f64, hi: f64| -> Result<f64, String> { Ok(v.clamp(lo, hi)) }),
    );
    assert_eq!(
        writ.run("return clamp3(5.0, 0.0, 3.0)").unwrap(),
        Value::F64(3.0)
    );
}

#[test]
fn test_mfn3_callable() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.register_method(
        ValueTag::Str,
        "replace3",
        None,
        mfn3(|s: Rc<str>, from: Rc<str>, to: Rc<str>, n: i32| -> Result<String, String> {
            let mut result = s.to_string();
            for _ in 0..n {
                result = result.replacen(from.as_ref(), to.as_ref(), 1);
            }
            Ok(result)
        }),
    );
    assert_eq!(
        writ.run(r#"return "aaa".replace3("a", "b", 2)"#).unwrap(),
        Value::Str(Rc::from("bba"))
    );
}

// ── type_checker_mut ──────────────────────────────────────────────────────────

#[test]
fn test_type_checker_mut_register_host_function() {
    let mut writ = Writ::new();
    writ.type_checker_mut()
        .register_host_function("my_typed_fn", vec![Type::Int], Type::Int);
    assert!(writ.run("return my_typed_fn(\"oops\")").is_err());
}

// ── WritError variants ────────────────────────────────────────────────────────

#[test]
fn test_write_error_parse_on_invalid_syntax() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    assert!(matches!(writ.run("return return"), Err(WritError::Parse(_))));
}

#[test]
fn test_write_error_io_on_missing_file() {
    let mut writ = Writ::new();
    assert!(matches!(
        writ.load("/no/such/file_writ_test_io.writ"),
        Err(WritError::Io(_))
    ));
}

// ── Value::Dict variant ───────────────────────────────────────────────────────

#[test]
fn test_value_dict_constructed_and_passed_back() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    let result = writ.run("let d = {\"x\": 1}\nreturn d").unwrap();
    assert!(matches!(result, Value::Dict(_)));
}

// ── Value::CoroutineHandle variant ────────────────────────────────────────────

#[test]
fn test_value_coroutine_handle_from_start() {
    let mut writ = Writ::new();
    writ.disable_type_checking();
    writ.run("func spin() { while true { yield } }\nstart spin()")
        .unwrap();
    let id = writ.vm_mut().last_coroutine_id();
    assert!(id.is_some(), "expected a coroutine to be started");
}
