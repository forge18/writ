use std::cell::{Cell, RefCell};
use std::rc::Rc;

use writ_compiler::{Chunk, CompiledFunction, Compiler};
use writ_lexer::Lexer;
use writ_parser::Parser;

use writ_vm::{BreakpointAction, RuntimeError, VM, Value, WritObject};

// ── Test helpers ────────────────────────────────────────────────────

/// Compiles a single expression and executes it, returning the result.
fn eval_expr(source: &str) -> Value {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_expr(&expr).expect("compile failed");
    let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();
    let mut vm = VM::new();
    vm.execute_program(&chunk, &functions, &struct_metas, &class_metas)
        .expect("vm failed")
}

/// Compiles a program and executes it, returning the result.
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

/// Compiles a program and expects a RuntimeError.
fn eval_error(source: &str) -> RuntimeError {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();
    let mut vm = VM::new();
    vm.execute_program(&chunk, &functions, &struct_metas, &class_metas)
        .expect_err("expected RuntimeError")
}

/// Compiles a program and sets the file name on all chunks.
fn compile_with_file(source: &str, file: &str) -> (Chunk, Vec<CompiledFunction>) {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    let (mut chunk, functions, _struct_metas, _class_metas) = compiler.into_parts();
    chunk.set_file(file);
    let functions: Vec<_> = functions
        .into_iter()
        .map(|mut f| {
            f.chunk.set_file(file);
            f
        })
        .collect();
    (chunk, functions)
}

// ── Literal tests ──────────────────────────────────────────────────

#[test]
fn test_execute_int_literal() {
    assert_eq!(eval_expr("42"), Value::I32(42));
    assert_eq!(eval_expr("0"), Value::I32(0));
    assert_eq!(eval_expr("-1"), Value::I32(-1));
}

// ── Arithmetic tests ───────────────────────────────────────────────

#[test]
fn test_execute_arithmetic() {
    assert_eq!(eval_expr("3 + 4"), Value::I32(7));
    assert_eq!(eval_expr("10 - 3"), Value::I32(7));
    assert_eq!(eval_expr("6 * 7"), Value::I32(42));
    assert_eq!(eval_expr("15 / 3"), Value::I32(5));
    assert_eq!(eval_expr("17 % 5"), Value::I32(2));
}

#[test]
fn test_execute_operator_precedence() {
    assert_eq!(eval_expr("2 + 3 * 4"), Value::I32(14));
    assert_eq!(eval_expr("(2 + 3) * 4"), Value::I32(20));
    assert_eq!(eval_expr("10 - 2 * 3"), Value::I32(4));
}

// ── Boolean logic tests ────────────────────────────────────────────

#[test]
fn test_execute_boolean_logic() {
    assert_eq!(eval_expr("true && false"), Value::Bool(false));
    assert_eq!(eval_expr("true && true"), Value::Bool(true));
    assert_eq!(eval_expr("false || true"), Value::Bool(true));
    assert_eq!(eval_expr("false || false"), Value::Bool(false));
    assert_eq!(eval_expr("!true"), Value::Bool(false));
    assert_eq!(eval_expr("!false"), Value::Bool(true));
}

// ── Comparison tests ───────────────────────────────────────────────

#[test]
fn test_execute_comparison() {
    assert_eq!(eval_expr("3 < 5"), Value::Bool(true));
    assert_eq!(eval_expr("5 < 3"), Value::Bool(false));
    assert_eq!(eval_expr("3 <= 3"), Value::Bool(true));
    assert_eq!(eval_expr("5 > 3"), Value::Bool(true));
    assert_eq!(eval_expr("3 >= 5"), Value::Bool(false));
    assert_eq!(eval_expr("3 == 3"), Value::Bool(true));
    assert_eq!(eval_expr("3 != 4"), Value::Bool(true));
    assert_eq!(eval_expr("3 == 4"), Value::Bool(false));
}

// ── Variable tests ─────────────────────────────────────────────────

#[test]
fn test_execute_let_and_load() {
    let result = eval(
        "func f() -> int {\n\
         let x = 42\n\
         return x\n\
         }\n\
         return f()",
    );
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_execute_var_and_assign() {
    let result = eval(
        "func f() -> int {\n\
         var x = 10\n\
         x = 20\n\
         return x\n\
         }\n\
         return f()",
    );
    assert_eq!(result, Value::I32(20));
}

// ── Control flow tests ─────────────────────────────────────────────

#[test]
fn test_execute_if_true() {
    let result = eval(
        "func f() -> int {\n\
         if true { return 1 }\n\
         return 0\n\
         }\n\
         return f()",
    );
    assert_eq!(result, Value::I32(1));
}

#[test]
fn test_execute_if_false() {
    let result = eval(
        "func f() -> int {\n\
         if false { return 1 }\n\
         return 0\n\
         }\n\
         return f()",
    );
    assert_eq!(result, Value::I32(0));
}

#[test]
fn test_execute_while_loop() {
    let result = eval(
        "func f() -> int {\n\
         var x = 0\n\
         while x < 5 { x += 1 }\n\
         return x\n\
         }\n\
         return f()",
    );
    assert_eq!(result, Value::I32(5));
}

#[test]
fn test_execute_for_range() {
    let result = eval(
        "func f() -> int {\n\
         var sum = 0\n\
         for i in 0..4 { sum += i }\n\
         return sum\n\
         }\n\
         return f()",
    );
    // 0 + 1 + 2 + 3 = 6
    assert_eq!(result, Value::I32(6));
}

// ── Function call tests ────────────────────────────────────────────

#[test]
fn test_execute_function_call() {
    let result = eval(
        "func add(a: int, b: int) -> int { return a + b }\n\
         return add(3, 4)",
    );
    assert_eq!(result, Value::I32(7));
}

#[test]
fn test_execute_recursive_function() {
    let result = eval(
        "func fib(n: int) -> int {\n\
         if n <= 1 { return n }\n\
         return fib(n - 1) + fib(n - 2)\n\
         }\n\
         return fib(6)",
    );
    // fib(6) = 8
    assert_eq!(result, Value::I32(8));
}

// ── String tests ───────────────────────────────────────────────────

#[test]
fn test_execute_string_concat() {
    // Test string addition via + operator
    let result = eval_expr(r#""hello " + "world""#);
    assert_eq!(result, Value::Str(Rc::new("hello world".to_string())));
}

// ── Collection tests ───────────────────────────────────────────────

#[test]
fn test_execute_array_literal() {
    let result = eval_expr("[1, 2, 3]");
    match result {
        Value::Array(arr) => {
            let items = arr.borrow();
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], Value::I32(1));
            assert_eq!(items[1], Value::I32(2));
            assert_eq!(items[2], Value::I32(3));
        }
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn test_execute_array_index() {
    // Use the for-in-array pattern to test GetIndex indirectly,
    // since direct arr[i] syntax may not be compiled to GetIndex yet.
    // Instead, build it via a function that sums array elements.
    let result = eval(
        "func f() -> int {\n\
         var sum = 0\n\
         for i in [10, 20, 30] { sum += i }\n\
         return sum\n\
         }\n\
         return f()",
    );
    // 10 + 20 + 30 = 60
    assert_eq!(result, Value::I32(60));
}

#[test]
fn test_execute_dict_literal() {
    let result = eval_expr(r#"{"a": 1, "b": 2}"#);
    match result {
        Value::Dict(dict) => {
            let entries = dict.borrow();
            assert_eq!(entries.len(), 2);
            assert_eq!(entries.get("a"), Some(&Value::I32(1)));
            assert_eq!(entries.get("b"), Some(&Value::I32(2)));
        }
        other => panic!("expected Dict, got {other:?}"),
    }
}

#[test]
fn test_execute_dict_access() {
    // Access dict field via dot notation: d.name
    let result = eval(
        "func f() -> string {\n\
         let d = {\"name\": \"Alice\"}\n\
         return d.name\n\
         }\n\
         return f()",
    );
    assert_eq!(result, Value::Str(Rc::new("Alice".to_string())));
}

// ── Return tests ───────────────────────────────────────────────────

#[test]
fn test_execute_return_from_function() {
    let result = eval(
        "func get() -> int { return 42 }\n\
         return get()",
    );
    assert_eq!(result, Value::I32(42));
}

// ── Error tests ────────────────────────────────────────────────────

#[test]
fn test_stack_trace_on_error() {
    let err = eval_error(
        "func bad() -> int { return 1 / 0 }\n\
         func caller() -> int { return bad() }\n\
         caller()",
    );
    assert!(err.message.contains("division by zero"));
    // Stack trace should have at least 2 frames: bad and caller
    assert!(
        err.trace.frames.len() >= 2,
        "expected at least 2 frames in stack trace, got {}",
        err.trace.frames.len()
    );
    assert_eq!(err.trace.frames[0].function, "bad");
    assert_eq!(err.trace.frames[1].function, "caller");
}

// ── Host interop helpers (Phase 12) ───────────────────────────────

/// Compiles and runs a program with a pre-configured VM.
fn eval_with_vm(source: &str, vm: &mut VM) -> Value {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    let (chunk, functions, _struct_metas, _class_metas) = compiler.into_parts();
    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed")
}

/// Compiles a program with a pre-configured VM and expects a RuntimeError.
fn eval_error_with_vm(source: &str, vm: &mut VM) -> RuntimeError {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    let (chunk, functions, _struct_metas, _class_metas) = compiler.into_parts();
    vm.execute_program(&chunk, &functions, &[], &[])
        .expect_err("expected RuntimeError")
}

/// A mock host-owned type for testing WritObject support.
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
            "name" => Ok(Value::Str(Rc::new(self.name.clone()))),
            "health" => Ok(Value::F32(self.health)),
            _ => Err(format!("Player has no field '{name}'")),
        }
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name {
            "health" => match value {
                Value::F32(v) => {
                    self.health = v;
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
            "greet" => Ok(Value::Str(Rc::new(format!("Hello, I'm {}!", self.name)))),
            _ => Err(format!("Player has no method '{name}'")),
        }
    }
}

// ── Host interop tests (Phase 12) ────────────────────────────────

#[test]
fn test_register_fn_callable() {
    let mut vm = VM::new();
    vm.register_fn("add_native", 2, |args| match (&args[0], &args[1]) {
        (Value::I32(a), Value::I32(b)) => {
            Ok(Value::I32(a + b))
        }
        _ => Err("expected two ints".to_string()),
    });
    let result = eval_with_vm("return add_native(3, 4)", &mut vm);
    assert_eq!(result, Value::I32(7));
}

#[test]
fn test_register_fn_wrong_arg_type() {
    let mut vm = VM::new();
    vm.register_fn("need_int", 1, |args| match &args[0] {
        Value::I32(v) => Ok(Value::I32(*v * 2)),
        other => Err(format!("expected int, got {}", other.type_name())),
    });
    let err = eval_error_with_vm(r#"return need_int("hello")"#, &mut vm);
    assert!(err.message.contains("expected int"));
}

#[test]
fn test_unregistered_fn_errors() {
    let mut vm = VM::new();
    let err = eval_error_with_vm("return unknown_fn(1)", &mut vm);
    assert!(err.message.contains("undefined function"));
    assert!(err.message.contains("unknown_fn"));
}

#[test]
fn test_register_type_field_access() {
    let player = MockPlayer {
        name: "Hero".to_string(),
        health: 100.0,
    };
    let player_obj: Value = Value::Object(Rc::new(RefCell::new(player)));

    let mut vm = VM::new();
    let obj_clone = player_obj.clone();
    vm.register_fn("get_player", 0, move |_args| Ok(obj_clone.clone()));

    let result = eval_with_vm(
        "func f() -> float {\n\
         let p = get_player()\n\
         return p.health\n\
         }\n\
         return f()",
        &mut vm,
    );
    assert_eq!(result, Value::F32(100.0));
}

#[test]
fn test_register_type_method_call() {
    let player = MockPlayer {
        name: "Hero".to_string(),
        health: 100.0,
    };
    let player_obj = Rc::new(RefCell::new(player));

    let mut vm = VM::new();
    let obj_ref = Rc::clone(&player_obj);
    vm.register_fn("greet_player", 0, move |_args| {
        obj_ref
            .borrow_mut()
            .call_method("greet", &[])
            .map_err(|e| e.to_string())
    });

    let result = eval_with_vm("return greet_player()", &mut vm);
    assert_eq!(result, Value::Str(Rc::new("Hello, I'm Hero!".to_string())));
}

#[test]
fn test_disable_module_blocks_calls() {
    let mut vm = VM::new();
    vm.register_fn_in_module("read_file", "io", 1, |_args| {
        Ok(Value::Str(Rc::new("file contents".to_string())))
    });
    vm.disable_module("io");

    let err = eval_error_with_vm(r#"return read_file("test.txt")"#, &mut vm);
    assert!(err.message.contains("disabled"));
    assert!(err.message.contains("io"));
}

#[test]
fn test_instruction_limit_exceeded() {
    let mut vm = VM::new();
    vm.set_instruction_limit(50);

    let err = eval_error_with_vm(
        "func f() -> int {\n\
         var x = 0\n\
         while true { x += 1 }\n\
         return x\n\
         }\n\
         return f()",
        &mut vm,
    );
    assert!(err.message.contains("instruction limit exceeded"));
}

#[test]
fn test_instruction_limit_reset_per_call() {
    let mut vm = VM::new();
    vm.set_instruction_limit(200);

    // First short program should succeed
    let result1 = eval_with_vm("return 1 + 2", &mut vm);
    assert_eq!(result1, Value::I32(3));

    // Second short program should also succeed (counter resets)
    let result2 = eval_with_vm("return 10 + 20", &mut vm);
    assert_eq!(result2, Value::I32(30));
}

#[test]
fn test_two_vms_isolated_registrations() {
    let mut vm1 = VM::new();
    vm1.register_fn("exclusive", 0, |_args| Ok(Value::I32(42)));

    let mut vm2 = VM::new();

    // VM1 can call the function
    let result = eval_with_vm("return exclusive()", &mut vm1);
    assert_eq!(result, Value::I32(42));

    // VM2 cannot — function is not registered there
    let err = eval_error_with_vm("return exclusive()", &mut vm2);
    assert!(err.message.contains("undefined function"));
    assert!(err.message.contains("exclusive"));
}

// ── Coroutine tests (Phase 13) ────────────────────────────────────

/// Helper: compile + execute_program, returning the VM for further ticking.
fn compile_and_run(source: &str, vm: &mut VM) -> Value {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    let (chunk, functions, _struct_metas, _class_metas) = compiler.into_parts();
    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed")
}

#[test]
fn test_coroutine_runs_across_frames() {
    // A coroutine with one yield completes after two ticks:
    // tick 1 = first run (hits yield), tick 2 = resume (completes).
    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("inc", 0, move |_args| {
        *counter_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func work() {\n\
         inc()\n\
         yield\n\
         inc()\n\
         }\n\
         start work()",
        &mut vm,
    );

    // After execute_program: coroutine is queued (Running), not yet executed.
    assert_eq!(vm.active_coroutine_count(), 1);
    assert_eq!(*counter.borrow(), 0);

    // Tick 1: coroutine first run — calls inc(), hits yield, suspends
    vm.tick(0.016).expect("tick 1 failed");
    assert_eq!(*counter.borrow(), 1);
    assert_eq!(vm.active_coroutine_count(), 1);

    // Tick 2: coroutine resumes after yield, calls second inc(), returns
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*counter.borrow(), 2);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_yield_one_frame() {
    // Bare `yield` suspends for exactly one frame.
    // tick 1 = first run (hits yield), tick 2 = resume (calls mark, completes).
    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("mark", 0, move |_args| {
        *counter_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func coro() {\n\
         yield\n\
         mark()\n\
         }\n\
         start coro()",
        &mut vm,
    );

    assert_eq!(*counter.borrow(), 0);

    // Tick 1: first run — hits yield, suspends (OneFrame)
    vm.tick(0.016).expect("tick 1 failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 2: resumes after yield, calls mark(), completes
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*counter.borrow(), 1);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_yield_seconds() {
    // yield waitForSeconds(1.0) resumes after sufficient delta accumulates.
    // tick 1 = first run (hits YieldSeconds, suspends with 1.0s remaining).
    // Subsequent ticks decrement the timer.
    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("done", 0, move |_args| {
        *counter_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func delayed() {\n\
         yield waitForSeconds(1.0)\n\
         done()\n\
         }\n\
         start delayed()",
        &mut vm,
    );

    // Tick 1: first run — hits YieldSeconds, suspends with 1.0s remaining
    vm.tick(0.016).expect("tick 1 (first run) failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 2: 1.0 - 0.5 = 0.5 remaining, not ready
    vm.tick(0.5).expect("tick 2 failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 3: 0.5 - 0.3 = 0.2 remaining, not ready
    vm.tick(0.3).expect("tick 3 failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 4: 0.2 - 0.3 = -0.1 ≤ 0, ready, resume → calls done()
    vm.tick(0.3).expect("tick 4 failed");
    assert_eq!(*counter.borrow(), 1);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_yield_frames() {
    // yield waitForFrames(3) resumes after 3 ticks (plus 1 for initial run).
    // tick 1 = first run (hits YieldFrames, suspends with 3 frames remaining).
    // ticks 2-4 count down the frames.
    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("done", 0, move |_args| {
        *counter_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func delayed() {\n\
         yield waitForFrames(3)\n\
         done()\n\
         }\n\
         start delayed()",
        &mut vm,
    );

    // Tick 1: first run — hits YieldFrames, suspends with remaining=3
    vm.tick(0.016).expect("tick 1 (first run) failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 2: 3 → 2, not ready
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 3: 2 → 1, not ready
    vm.tick(0.016).expect("tick 3 failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 4: 1 → 0, ready, resume → calls done()
    vm.tick(0.016).expect("tick 4 failed");
    assert_eq!(*counter.borrow(), 1);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_yield_until_condition() {
    // yield waitUntil(predicate) resumes when predicate returns true.
    // tick 1 = first run (hits YieldUntil, suspends).
    // Subsequent ticks evaluate the predicate.
    let flag = Rc::new(RefCell::new(false));
    let flag_check = Rc::clone(&flag);
    let flag_set = Rc::clone(&flag);

    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("is_ready", 0, move |_args| {
        Ok(Value::Bool(*flag_check.borrow()))
    });
    vm.register_fn("done", 0, move |_args| {
        *counter_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func waiter() {\n\
         yield waitUntil(is_ready)\n\
         done()\n\
         }\n\
         start waiter()",
        &mut vm,
    );

    // Tick 1: first run — hits YieldUntil, suspends
    vm.tick(0.016).expect("tick 1 (first run) failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 2: predicate false — stays suspended
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*counter.borrow(), 0);

    // Tick 3: still false
    vm.tick(0.016).expect("tick 3 failed");
    assert_eq!(*counter.borrow(), 0);

    // Set flag to true, tick — predicate met, resumes, calls done()
    *flag_set.borrow_mut() = true;
    vm.tick(0.016).expect("tick 4 failed");
    assert_eq!(*counter.borrow(), 1);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_yield_coroutine_waits_for_child() {
    // yield child_fn() suspends parent until child completes.
    // tick 1: parent first run → starts child, yields waiting for child
    // tick 2: child first run → hits yield, suspends
    // tick 3: child resumes → calls log_child(), completes
    // tick 4: parent sees child done → resumes, calls log_parent(), completes
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let log1 = Rc::clone(&log);
    let log2 = Rc::clone(&log);

    let mut vm = VM::new();
    vm.register_fn("log_parent", 0, move |_args| {
        log1.borrow_mut().push("parent".to_string());
        Ok(Value::Null)
    });
    vm.register_fn("log_child", 0, move |_args| {
        log2.borrow_mut().push("child".to_string());
        Ok(Value::Null)
    });

    compile_and_run(
        "func child_fn() {\n\
         yield\n\
         log_child()\n\
         }\n\
         func parent_fn() {\n\
         yield child_fn()\n\
         log_parent()\n\
         }\n\
         start parent_fn()",
        &mut vm,
    );

    // Tick 1: parent first run → starts child coroutine, yields (WaitCoroutine)
    vm.tick(0.016).expect("tick 1 failed");
    assert_eq!(*log.borrow(), Vec::<String>::new());

    // Tick 2: child first run → hits yield, suspends (OneFrame)
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*log.borrow(), Vec::<String>::new());

    // Tick 3: child resumes → calls log_child(), completes
    vm.tick(0.016).expect("tick 3 failed");
    assert_eq!(*log.borrow(), vec!["child"]);

    // Tick 4: parent sees child complete → resumes, calls log_parent(), completes
    vm.tick(0.016).expect("tick 4 failed");
    assert_eq!(*log.borrow(), vec!["child", "parent"]);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_coroutine_return_value() {
    // Child's return value is received by parent via yield.
    // tick 1: consumer first run → starts compute, yields (WaitCoroutine)
    // tick 2: compute first run → hits yield, suspends (OneFrame)
    // tick 3: compute resumes → returns 42, completes
    // tick 4: consumer sees child complete → resumes with val=42, stores it
    let result = Rc::new(RefCell::new(Value::Null));
    let result_ref = Rc::clone(&result);

    let mut vm = VM::new();
    vm.register_fn("store_result", 1, move |args| {
        *result_ref.borrow_mut() = args[0].clone();
        Ok(Value::Null)
    });

    compile_and_run(
        "func compute() -> int {\n\
         yield\n\
         return 42\n\
         }\n\
         func consumer() {\n\
         let val = yield compute()\n\
         store_result(val)\n\
         }\n\
         start consumer()",
        &mut vm,
    );

    // Tick 1: consumer first run → starts compute child, yields waiting for child
    vm.tick(0.016).expect("tick 1 failed");
    assert_eq!(*result.borrow(), Value::Null);

    // Tick 2: compute first run → hits yield, suspends
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*result.borrow(), Value::Null);

    // Tick 3: compute resumes → returns 42, completes
    vm.tick(0.016).expect("tick 3 failed");
    assert_eq!(*result.borrow(), Value::Null);

    // Tick 4: consumer sees child complete with 42, resumes with val=42, stores it
    vm.tick(0.016).expect("tick 4 failed");
    assert_eq!(*result.borrow(), Value::I32(42));
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_coroutine_cancel_on_owner_destroy() {
    // cancel_coroutines_for(owner) stops the coroutine.
    // tick 1: first run — calls inc(), hits yield, suspends
    // cancel between ticks
    // tick 2: cancelled, no more work
    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("inc", 0, move |_args| {
        *counter_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func looper() {\n\
         inc()\n\
         yield\n\
         inc()\n\
         yield\n\
         inc()\n\
         }\n\
         start looper()",
        &mut vm,
    );

    let coro_id = vm.last_coroutine_id().expect("should have a coroutine");
    vm.set_coroutine_owner(coro_id, 999);

    // Tick 1: first run — calls inc(), hits yield, suspends
    vm.tick(0.016).expect("tick 1 failed");
    assert_eq!(*counter.borrow(), 1);

    // Cancel all coroutines owned by object 999
    vm.cancel_coroutines_for(999);

    // Tick 2: coroutine is cancelled, no more inc() calls, gets cleaned up
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*counter.borrow(), 1);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_coroutine_cancel_propagates_to_child() {
    // Cancelling parent cancels its children.
    // tick 1: parent first run → calls parent_work(), starts child, yields (WaitCoroutine)
    //         child not yet ticked (created during parent's resume)
    // tick 2: child first run → calls child_work(), hits yield, suspends
    // cancel between ticks → cancels parent + propagates to child
    // tick 3: both cancelled, no more work
    let parent_log = Rc::new(RefCell::new(0i32));
    let child_log = Rc::new(RefCell::new(0i32));
    let parent_ref = Rc::clone(&parent_log);
    let child_ref = Rc::clone(&child_log);

    let mut vm = VM::new();
    vm.register_fn("parent_work", 0, move |_args| {
        *parent_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });
    vm.register_fn("child_work", 0, move |_args| {
        *child_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func child_fn() {\n\
         child_work()\n\
         yield\n\
         child_work()\n\
         }\n\
         func parent_fn() {\n\
         parent_work()\n\
         yield child_fn()\n\
         parent_work()\n\
         }\n\
         start parent_fn()",
        &mut vm,
    );

    // Set owner before first tick
    let parent_coro_id = vm
        .last_coroutine_id()
        .expect("should have parent coroutine");
    vm.set_coroutine_owner(parent_coro_id, 42);

    // Tick 1: parent first run → calls parent_work(), starts child, yields
    vm.tick(0.016).expect("tick 1 failed");
    assert_eq!(*parent_log.borrow(), 1);
    assert_eq!(*child_log.borrow(), 0); // child not run yet

    // Tick 2: child first run → calls child_work(), hits yield, suspends
    vm.tick(0.016).expect("tick 2 failed");
    assert_eq!(*child_log.borrow(), 1);

    // Cancel all coroutines owned by object 42 — should cancel parent + child
    vm.cancel_coroutines_for(42);

    // Tick 3: both cancelled, cleaned up
    vm.tick(0.016).expect("tick 3 failed");
    assert_eq!(*parent_log.borrow(), 1);
    assert_eq!(*child_log.borrow(), 1);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_multiple_coroutines_run_concurrently() {
    // 3 coroutines started with `start`, all run on each tick.
    let counter = Rc::new(RefCell::new(0i32));
    let c1 = Rc::clone(&counter);
    let c2 = Rc::clone(&counter);
    let c3 = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("inc_a", 0, move |_args| {
        *c1.borrow_mut() += 1;
        Ok(Value::Null)
    });
    vm.register_fn("inc_b", 0, move |_args| {
        *c2.borrow_mut() += 1;
        Ok(Value::Null)
    });
    vm.register_fn("inc_c", 0, move |_args| {
        *c3.borrow_mut() += 1;
        Ok(Value::Null)
    });

    compile_and_run(
        "func a() { inc_a() }\n\
         func b() { inc_b() }\n\
         func c() { inc_c() }\n\
         start a()\n\
         start b()\n\
         start c()",
        &mut vm,
    );

    assert_eq!(vm.active_coroutine_count(), 3);

    // Single tick: all three run and complete
    vm.tick(0.016).expect("tick failed");
    assert_eq!(*counter.borrow(), 3);
    assert_eq!(vm.active_coroutine_count(), 0);
}

#[test]
fn test_start_does_not_block() {
    // `start foo()` returns immediately; main script continues.
    let counter = Rc::new(RefCell::new(0i32));
    let counter_ref = Rc::clone(&counter);

    let mut vm = VM::new();
    vm.register_fn("inc", 0, move |_args| {
        *counter_ref.borrow_mut() += 1;
        Ok(Value::Null)
    });

    // The main script starts a coroutine then returns 99.
    // The coroutine should NOT have run yet.
    let result = compile_and_run(
        "func work() {\n\
         inc()\n\
         yield\n\
         inc()\n\
         }\n\
         start work()\n\
         return 99",
        &mut vm,
    );

    // Main script returned immediately
    assert_eq!(result, Value::I32(99));
    // Coroutine has not been ticked yet — inc() not called
    assert_eq!(*counter.borrow(), 0);
    // But it's queued
    assert_eq!(vm.active_coroutine_count(), 1);
}

// ── Phase 14: Debug features ──────────────────────────────────────

#[test]
fn test_stack_trace_single_frame() {
    let (chunk, functions) = compile_with_file("1 / 0", "test.writ");
    let mut vm = VM::new();
    let err = vm
        .execute_program(&chunk, &functions, &[], &[])
        .expect_err("expected RuntimeError");
    assert!(err.message.contains("division by zero"));
    assert_eq!(err.trace.frames.len(), 1);
    assert_eq!(err.trace.frames[0].function, "<script>");
    assert_eq!(err.trace.frames[0].file, "test.writ");
    assert!(err.trace.frames[0].line > 0);
    assert!(!err.trace.frames[0].is_native);
}

#[test]
fn test_stack_trace_multiple_frames() {
    let (chunk, functions) = compile_with_file(
        "func c() -> int { return 1 / 0 }\n\
         func b() -> int { return c() }\n\
         func a() -> int { return b() }\n\
         a()",
        "multi.writ",
    );
    let mut vm = VM::new();
    let err = vm
        .execute_program(&chunk, &functions, &[], &[])
        .expect_err("expected RuntimeError");
    assert!(err.trace.frames.len() >= 3);
    assert_eq!(err.trace.frames[0].function, "c");
    assert_eq!(err.trace.frames[0].file, "multi.writ");
    assert_eq!(err.trace.frames[1].function, "b");
    assert_eq!(err.trace.frames[1].file, "multi.writ");
    assert_eq!(err.trace.frames[2].function, "a");
    assert_eq!(err.trace.frames[2].file, "multi.writ");
}

#[test]
fn test_stack_trace_lambda() {
    let (chunk, functions) = compile_with_file(
        "let bad = (x: int) => { return x / 0 }\nbad(1)",
        "lambda.writ",
    );
    let mut vm = VM::new();
    let err = vm
        .execute_program(&chunk, &functions, &[], &[])
        .expect_err("expected RuntimeError");
    // The lambda frame should show <lambda>, not __lambda_0
    let lambda_frame = err.trace.frames.iter().find(|f| f.function == "<lambda>");
    assert!(
        lambda_frame.is_some(),
        "expected a <lambda> frame in stack trace, got: {:?}",
        err.trace
            .frames
            .iter()
            .map(|f| &f.function)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_breakpoint_fires() {
    let fired = Rc::new(Cell::new(false));
    let bp_line = Rc::new(Cell::new(0u32));
    let bp_file = Rc::new(RefCell::new(String::new()));

    let fired_c = fired.clone();
    let bp_line_c = bp_line.clone();
    let bp_file_c = bp_file.clone();

    let (chunk, functions) = compile_with_file("let x = 1\nlet y = 2\nlet z = x + y", "bp.writ");

    let mut vm = VM::new();
    vm.set_breakpoint("bp.writ", 2);
    vm.on_breakpoint(move |ctx| {
        fired_c.set(true);
        bp_line_c.set(ctx.line);
        *bp_file_c.borrow_mut() = ctx.file.to_string();
        BreakpointAction::Continue
    });

    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed");
    assert!(fired.get(), "breakpoint handler should have been called");
    assert_eq!(bp_line.get(), 2);
    assert_eq!(*bp_file.borrow(), "bp.writ");
}

#[test]
fn test_breakpoint_continue() {
    let (chunk, functions) = compile_with_file(
        "func add() -> int {\n  let x = 10\n  let y = 20\n  return x + y\n}\nreturn add()",
        "cont.writ",
    );

    let mut vm = VM::new();
    vm.set_breakpoint("cont.writ", 3);
    vm.on_breakpoint(|_ctx| BreakpointAction::Continue);

    let result = vm
        .execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed");
    assert_eq!(result, Value::I32(30));
}

#[test]
fn test_breakpoint_not_set_does_not_fire() {
    let fired = Rc::new(Cell::new(false));
    let fired_c = fired.clone();

    let (chunk, functions) = compile_with_file("let x = 1\nlet y = 2\nx + y", "no_bp.writ");

    let mut vm = VM::new();
    // Register handler but set NO breakpoints
    vm.on_breakpoint(move |_ctx| {
        fired_c.set(true);
        BreakpointAction::Continue
    });

    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed");
    assert!(
        !fired.get(),
        "breakpoint handler should NOT have been called"
    );
}

#[test]
fn test_debug_hook_on_line() {
    let lines = Rc::new(RefCell::new(Vec::<u32>::new()));
    let lines_c = lines.clone();

    let (chunk, functions) = compile_with_file("let x = 1\nlet y = 2\nlet z = 3", "lines.writ");

    let mut vm = VM::new();
    vm.on_line(move |_file, line| {
        lines_c.borrow_mut().push(line);
    });

    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed");
    let recorded = lines.borrow();
    // Should have recorded at least 3 distinct lines (1, 2, 3)
    assert!(
        recorded.len() >= 3,
        "expected at least 3 on_line calls, got {}: {:?}",
        recorded.len(),
        *recorded
    );
    assert!(recorded.contains(&1));
    assert!(recorded.contains(&2));
    assert!(recorded.contains(&3));
}

#[test]
fn test_debug_hook_on_call() {
    let calls = Rc::new(RefCell::new(Vec::<String>::new()));
    let calls_c = calls.clone();

    let (chunk, functions) =
        compile_with_file("func greet() -> int { return 42 }\ngreet()", "calls.writ");

    let mut vm = VM::new();
    vm.on_call(move |fn_name, _file, _line| {
        calls_c.borrow_mut().push(fn_name.to_string());
    });

    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed");
    let recorded = calls.borrow();
    assert!(
        recorded.contains(&"greet".to_string()),
        "expected 'greet' in on_call records, got: {:?}",
        *recorded
    );
}

#[test]
fn test_debug_hook_on_return() {
    let returns = Rc::new(RefCell::new(Vec::<String>::new()));
    let returns_c = returns.clone();

    let (chunk, functions) =
        compile_with_file("func greet() -> int { return 42 }\ngreet()", "returns.writ");

    let mut vm = VM::new();
    vm.on_return(move |fn_name, _file, _line| {
        returns_c.borrow_mut().push(fn_name.to_string());
    });

    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("vm failed");
    let recorded = returns.borrow();
    assert!(
        recorded.contains(&"greet".to_string()),
        "expected 'greet' in on_return records, got: {:?}",
        *recorded
    );
}

#[test]
fn test_hot_reload_updates_function() {
    // First: load a program with a function that returns 1
    let (chunk, functions) = compile_with_file("func get() -> int { return 1 }", "reload.writ");

    let mut vm = VM::new();
    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("initial run failed");

    // Verify original function
    let result = vm.call_function("get", &[]).expect("call_function failed");
    assert_eq!(result, Value::I32(1));

    // Reload with updated function
    vm.reload("reload.writ", "func get() -> int { return 42 }")
        .expect("reload failed");

    // Verify updated function
    let result = vm
        .call_function("get", &[])
        .expect("call_function after reload failed");
    assert_eq!(result, Value::I32(42));
}

#[test]
fn test_hot_reload_preserves_state() {
    // Load a program that defines two functions
    let (chunk, functions) = compile_with_file(
        "func get() -> int { return 1 }\n\
         func other() -> int { return 100 }",
        "state.writ",
    );

    let mut vm = VM::new();
    // Register a native function to verify VM registrations survive reload
    let counter = Rc::new(Cell::new(0i32));
    let counter_c = counter.clone();
    vm.register_fn("inc", 0, move |_args| {
        let v = counter_c.get() + 1;
        counter_c.set(v);
        Ok(Value::I32(v))
    });

    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("initial run failed");

    // Verify both functions work
    assert_eq!(
        vm.call_function("get", &[]).unwrap(),
        Value::I32(1)
    );
    assert_eq!(
        vm.call_function("other", &[]).unwrap(),
        Value::I32(100)
    );

    // Reload only changes "get", "other" should be preserved
    vm.reload("state.writ", "func get() -> int { return 99 }")
        .expect("reload failed");

    // Reloaded function returns new value
    assert_eq!(
        vm.call_function("get", &[]).unwrap(),
        Value::I32(99)
    );
    // Unreloaded function still returns original value
    assert_eq!(
        vm.call_function("other", &[]).unwrap(),
        Value::I32(100)
    );
}

#[test]
fn test_hot_reload_compile_error_preserves_previous() {
    let (chunk, functions) = compile_with_file("func get() -> int { return 1 }", "error.writ");

    let mut vm = VM::new();
    vm.execute_program(&chunk, &functions, &[], &[])
        .expect("initial run failed");

    // Verify original function
    let result = vm.call_function("get", &[]).expect("call_function failed");
    assert_eq!(result, Value::I32(1));

    // Attempt reload with invalid source — should fail
    let err = vm.reload("error.writ", "func get( { broken syntax");
    assert!(err.is_err(), "reload should have failed on bad syntax");

    // Original function should still work
    let result = vm
        .call_function("get", &[])
        .expect("call_function after failed reload");
    assert_eq!(result, Value::I32(1));
}

// ══════════════════════════════════════════════════════════════════════
// Numeric promotion tests
// ══════════════════════════════════════════════════════════════════════

#[test]
fn test_int_promotion_on_overflow() {
    // i32 max is 2147483647; adding 1 should promote to i64.
    let result = eval("return 2147483647 + 1");
    assert_eq!(result, Value::I64(2_147_483_648));
}

#[test]
fn test_int_promotion_on_underflow() {
    // i32 min is -2147483648; subtracting 1 should promote to i64.
    let result = eval("var x = -2147483648\nreturn x - 1");
    assert_eq!(result, Value::I64(-2_147_483_649));
}

#[test]
fn test_int_promotion_on_multiply_overflow() {
    // 50000 * 50000 = 2_500_000_000, which overflows i32.
    let result = eval_expr("50000 * 50000");
    assert_eq!(result, Value::I64(2_500_000_000));
}

#[test]
fn test_large_int_literal_stays_i64() {
    // A literal that doesn't fit i32 should be i64 from the start.
    let result = eval("return 9999999999999");
    assert_eq!(result, Value::I64(9_999_999_999_999));
}

#[test]
fn test_mixed_width_int_arithmetic() {
    // i32 + i64 should produce i64.
    let result = eval("return 1 + 9999999999999");
    assert_eq!(result, Value::I64(10_000_000_000_000));
}

#[test]
fn test_small_int_stays_i32() {
    // Normal arithmetic that fits i32 should stay i32.
    let result = eval_expr("100 + 200");
    assert_eq!(result, Value::I32(300));
}

#[test]
fn test_negation_promotion() {
    // Negating i32::MIN (-2147483648) overflows i32; should promote to i64.
    let result = eval("var x = -2147483648\nreturn -x");
    assert_eq!(result, Value::I64(2_147_483_648));
}

#[test]
fn test_cross_width_equality() {
    // i32(42) should equal i64(42).
    let result = eval("return 42 == 9999999999999 - 9999999999957");
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_int_comparison_across_widths() {
    // Comparing i32 with i64 should work correctly.
    let result = eval("return 1 < 9999999999999");
    assert_eq!(result, Value::Bool(true));
}
