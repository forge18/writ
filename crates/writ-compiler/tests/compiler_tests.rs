use writ_compiler::{CompileError, Compiler, Instruction, string_hash};
use writ_lexer::Lexer;
use writ_parser::Parser;

// ── Test helpers ────────────────────────────────────────────────────

/// Compiles a single expression into register 0 and returns the instructions.
fn compile_expr(source: &str) -> Vec<Instruction> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler
        .compile_expr(&expr, Some(0))
        .expect("compile_expr failed");
    compiler.into_chunk().instructions().to_vec()
}

/// Compiles a program (sequence of statements) and returns the instructions.
fn compile(source: &str) -> Vec<Instruction> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    for stmt in &stmts {
        compiler.compile_stmt(stmt).expect("compile failed");
    }
    compiler.into_chunk().instructions().to_vec()
}

/// Compiles a program and returns the full Compiler (for accessing functions).
fn compile_full(source: &str) -> Compiler {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    for stmt in &stmts {
        compiler.compile_stmt(stmt).expect("compile failed");
    }
    compiler
}

/// Compiles a program and expects a CompileError.
fn compile_error(source: &str) -> CompileError {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    let mut err = None;
    for stmt in &stmts {
        if let Err(e) = compiler.compile_stmt(stmt) {
            err = Some(e);
            break;
        }
    }
    err.expect("expected CompileError")
}

// ── Literal tests ─────────────────────────────────────────────────

#[test]
fn test_emit_int_literal() {
    let instrs = compile_expr("42");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(0, 42))),
        "expected LoadInt(0, 42), got: {instrs:?}"
    );
}

#[test]
fn test_emit_float_literal() {
    let instrs = compile_expr("3.14");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadFloat(0, v) if (*v - 3.14_f32).abs() < 0.001)),
        "expected LoadFloat(0, 3.14), got: {instrs:?}"
    );
}

#[test]
fn test_emit_bool_literal() {
    let instrs = compile_expr("true");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadBool(0, true))),
        "expected LoadBool(0, true), got: {instrs:?}"
    );

    let instrs = compile_expr("false");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadBool(0, false))),
        "expected LoadBool(0, false), got: {instrs:?}"
    );
}

#[test]
fn test_emit_string_literal() {
    let instrs = compile_expr(r#""hello""#);
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadStr(0, _))),
        "expected LoadStr(0, _), got: {instrs:?}"
    );
}

#[test]
fn test_emit_null_literal() {
    let instrs = compile_expr("null");
    assert!(
        instrs.iter().any(|i| matches!(i, Instruction::LoadNull(0))),
        "expected LoadNull(0), got: {instrs:?}"
    );
}

// ── Arithmetic / constant folding tests ─────────────────────────────

#[test]
fn test_constant_fold_add() {
    // 1 + 2 is constant-folded to 3
    let instrs = compile_expr("1 + 2");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(0, 3))),
        "expected folded LoadInt(0, 3), got: {instrs:?}"
    );
}

#[test]
fn test_constant_fold_all_binary_ops() {
    let cases: &[(&str, i32)] = &[
        ("1 + 2", 3),
        ("1 - 2", -1),
        ("1 * 2", 2),
        ("1 / 2", 0), // integer division
        ("1 % 2", 1),
    ];
    for (source, expected) in cases {
        let instrs = compile_expr(source);
        assert!(
            instrs
                .iter()
                .any(|i| matches!(i, Instruction::LoadInt(0, v) if *v == *expected)),
            "failed for {source}: expected LoadInt(0, {expected}), got: {instrs:?}"
        );
    }
}

#[test]
fn test_constant_fold_bool_ops() {
    let cases: &[(&str, bool)] = &[
        ("1 == 2", false),
        ("1 != 2", true),
        ("1 < 2", true),
        ("1 > 2", false),
        ("1 <= 2", true),
        ("1 >= 2", false),
    ];
    for (source, expected) in cases {
        let instrs = compile_expr(source);
        assert!(
            instrs
                .iter()
                .any(|i| matches!(i, Instruction::LoadBool(0, v) if *v == *expected)),
            "failed for {source}: expected LoadBool(0, {expected}), got: {instrs:?}"
        );
    }
}

#[test]
fn test_constant_fold_unary_negate() {
    // -42 is constant-folded to LoadInt(_, -42)
    let instrs = compile_expr("-42");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(0, -42))),
        "expected folded LoadInt(0, -42), got: {instrs:?}"
    );
}

#[test]
fn test_constant_fold_unary_not() {
    // !true is constant-folded to LoadBool(_, false)
    let instrs = compile_expr("!true");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadBool(0, false))),
        "expected folded LoadBool(0, false), got: {instrs:?}"
    );
}

#[test]
fn test_emit_operator_precedence() {
    // 1 + 2 * 3: the 2*3 is folded to 6, then 1+6 uses AddInt
    let instrs = compile_expr("1 + 2 * 3");
    // Should have a typed AddInt somewhere (since both sides are known int)
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::AddInt(_, _, _))),
        "expected AddInt for int+int, got: {instrs:?}"
    );
}

// ── Variable declaration tests ─────────────────────────────────────

#[test]
fn test_emit_let_decl() {
    let instrs = compile("let x = 42");
    // In register model, `let x = 42` just loads 42 into the register assigned to x
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 42))),
        "expected LoadInt for let x = 42, got: {instrs:?}"
    );
}

#[test]
fn test_emit_multiple_locals() {
    let instrs = compile("let x = 1\nlet y = 2\nlet z = 3");
    // Each variable gets its own register with direct load
    let load_1 = instrs
        .iter()
        .any(|i| matches!(i, Instruction::LoadInt(_, 1)));
    let load_2 = instrs
        .iter()
        .any(|i| matches!(i, Instruction::LoadInt(_, 2)));
    let load_3 = instrs
        .iter()
        .any(|i| matches!(i, Instruction::LoadInt(_, 3)));
    assert!(
        load_1 && load_2 && load_3,
        "expected LoadInt for 1, 2, and 3, got: {instrs:?}"
    );
}

// ── Assignment tests ───────────────────────────────────────────────

#[test]
fn test_emit_assignment() {
    let instrs = compile("var x = 0\nx = 5");
    // Assignment writes 5 directly into x's register
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 5))),
        "expected LoadInt for x = 5, got: {instrs:?}"
    );
}

#[test]
fn test_emit_compound_assignment() {
    // x += 1 uses AddIntImm for register-based VM
    let instrs = compile("var x = 10\nx += 1");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::AddIntImm(_, _, 1))),
        "expected AddIntImm(_, _, 1) for x += 1, got: {instrs:?}"
    );
}

#[test]
fn test_emit_compound_sub() {
    let instrs = compile("var x = 10\nx -= 1");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::AddIntImm(_, _, -1))),
        "expected AddIntImm(_, _, -1) for x -= 1, got: {instrs:?}"
    );
}

// ── String deduplication test ──────────────────────────────────────

#[test]
fn test_string_deduplication() {
    let mut lexer = Lexer::new(
        r#"let x = "hello"
let y = "hello""#,
    );
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    for stmt in &stmts {
        compiler.compile_stmt(stmt).expect("compile failed");
    }
    let chunk = compiler.into_chunk();
    // Both should use LoadStr with the same string pool index
    let str_instrs: Vec<_> = chunk
        .instructions()
        .iter()
        .filter(|i| matches!(i, Instruction::LoadStr(_, _)))
        .collect();
    assert_eq!(str_instrs.len(), 2, "expected 2 LoadStr instructions");
    // Both should reference the same pool index
    if let (Instruction::LoadStr(_, idx1), Instruction::LoadStr(_, idx2)) =
        (str_instrs[0], str_instrs[1])
    {
        assert_eq!(idx1, idx2, "duplicate strings should share pool index");
    }
    assert_eq!(chunk.strings(), &["hello"]);
}

// ── Large integer test ────────────────────────────────────────────

#[test]
fn test_large_int_emits_load_const_int() {
    let instrs = compile("let x = 9999999999999");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadConstInt(_, 0))),
        "expected LoadConstInt(_, 0) for large int, got: {instrs:?}"
    );
}

// ── if/else tests ──────────────────────────────────────────────────

#[test]
fn test_compile_if_true_branch() {
    let instrs = compile("if true { let x = 1 }");
    // Should have a LoadBool for condition and a JumpIfFalsy for the branch
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadBool(_, true))),
        "expected LoadBool(_, true), got: {instrs:?}"
    );
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::JumpIfFalsy(_, _))),
        "expected JumpIfFalsy, got: {instrs:?}"
    );
}

#[test]
fn test_compile_if_else() {
    let instrs = compile("let x = true\nif x { let a = 1 } else { let b = 2 }");
    // Should have JumpIfFalsy and Jump (to skip else)
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::JumpIfFalsy(_, _))),
        "expected JumpIfFalsy, got: {instrs:?}"
    );
    assert!(
        instrs.iter().any(|i| matches!(i, Instruction::Jump(_))),
        "expected Jump to skip else, got: {instrs:?}"
    );
    // Both branches' values present
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 1)))
    );
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 2)))
    );
}

// ── while loop tests ───────────────────────────────────────────────

#[test]
fn test_compile_while() {
    let instrs = compile("var x = 0\nwhile x < 10 { x += 1 }");
    // Should have a comparison instruction and a backward jump
    let has_compare = instrs.iter().any(|i| {
        matches!(
            i,
            Instruction::LtInt(_, _, _)
                | Instruction::TestLtInt(_, _, _)
                | Instruction::TestLtIntImm(_, _, _)
        )
    });
    assert!(
        has_compare,
        "expected int comparison in while condition, got: {instrs:?}"
    );
    // Should have a backward (negative) jump
    let has_backward_jump = instrs
        .iter()
        .any(|i| matches!(i, Instruction::Jump(o) if *o < 0));
    assert!(
        has_backward_jump,
        "expected backward jump for while loop, got: {instrs:?}"
    );
}

// ── for loop tests ─────────────────────────────────────────────────

#[test]
fn test_compile_for_range() {
    let instrs = compile("for i in 0..3 { i }");
    // Should have loop setup with 0 and 3 as bounds
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 0)))
    );
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 3)))
    );
    // Should have a backward jump for the loop
    let has_backward_jump = instrs
        .iter()
        .any(|i| matches!(i, Instruction::Jump(o) if *o < 0));
    assert!(
        has_backward_jump,
        "expected backward jump for for-range, got: {instrs:?}"
    );
}

#[test]
fn test_compile_for_in_array() {
    let instrs = compile("let arr = [1, 2, 3]\nfor item in arr { item }");
    // Should have a MakeArray for the literal
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::MakeArray(_, _, 3))),
        "expected MakeArray(_, _, 3), got: {instrs:?}"
    );
    // Should have GetField for "length"
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::GetField(_, _, h) if *h == string_hash("length"))),
        "expected GetField for 'length', got: {instrs:?}"
    );
}

// ── when statement tests ───────────────────────────────────────────

#[test]
fn test_compile_when_value() {
    let instrs = compile("let x = 1\nwhen x { 0 => 42 }");
    // Should compare against 0
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 0)))
    );
    // Should have the arm body value 42
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 42)))
    );
    // Should have an equality test
    let has_eq = instrs.iter().any(|i| {
        matches!(
            i,
            Instruction::Eq(_, _, _)
                | Instruction::EqInt(_, _, _)
                | Instruction::TestEqInt(_, _, _)
        )
    });
    assert!(
        has_eq,
        "expected equality check in when arm, got: {instrs:?}"
    );
}

#[test]
fn test_compile_when_else() {
    let instrs = compile("let x = 1\nwhen x { 0 => 10; else => 20 }");
    // Both arm values present
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 10)))
    );
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 20)))
    );
}

// ── function declaration tests ─────────────────────────────────────

#[test]
fn test_compile_function_decl() {
    let compiler = compile_full("func add(a: int, b: int) -> int { return a + b }");
    let functions = compiler.functions();
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "add");
    assert_eq!(functions[0].arity, 2);
    // With typed parameters (a: int, b: int), the compiler emits AddInt
    let func_instrs = functions[0].chunk.instructions();
    assert!(
        func_instrs
            .iter()
            .any(|i| matches!(i, Instruction::AddInt(_, _, _))),
        "expected AddInt for int+int, got: {func_instrs:?}"
    );
    assert!(
        func_instrs
            .iter()
            .any(|i| matches!(i, Instruction::Return(_))),
        "expected Return, got: {func_instrs:?}"
    );
}

#[test]
fn test_compile_function_call() {
    let instrs = compile("func foo() { }\nfoo()");
    // foo() is a known compiled function → CallDirect with func_idx=0
    let has_call = instrs
        .iter()
        .any(|i| matches!(i, Instruction::CallDirect(_, 0, 0)));
    assert!(
        has_call,
        "should emit CallDirect(_, 0, 0) for foo(), got: {instrs:?}"
    );
}

#[test]
fn test_compile_return_value() {
    let compiler = compile_full("func get() -> int { return 42 }");
    let functions = compiler.functions();
    assert_eq!(functions.len(), 1);
    let func_instrs = functions[0].chunk.instructions();
    assert!(
        func_instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 42)))
    );
    assert!(
        func_instrs
            .iter()
            .any(|i| matches!(i, Instruction::Return(_)))
    );
}

#[test]
fn test_compile_function_implicit_return() {
    let compiler = compile_full("func noop() { }");
    let functions = compiler.functions();
    assert_eq!(functions.len(), 1);
    let func_instrs = functions[0].chunk.instructions();
    // Should have implicit ReturnNull
    assert!(
        func_instrs
            .iter()
            .any(|i| matches!(i, Instruction::ReturnNull)),
        "expected ReturnNull for implicit return, got: {func_instrs:?}"
    );
}

// ── collection literal tests ───────────────────────────────────────

#[test]
fn test_compile_array_literal() {
    let instrs = compile_expr("[1, 2, 3]");
    // Should have MakeArray with count 3
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::MakeArray(_, _, 3))),
        "expected MakeArray(_, _, 3), got: {instrs:?}"
    );
}

#[test]
fn test_compile_dict_literal() {
    let instrs = compile_expr(r#"{"a": 1, "b": 2}"#);
    // Should have MakeDict with 2 entries
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::MakeDict(_, _, 2))),
        "expected MakeDict(_, _, 2), got: {instrs:?}"
    );
}

// ── string interpolation tests ─────────────────────────────────────

#[test]
fn test_compile_string_concat() {
    let instrs = compile(
        r#"let name = "world"
"hello $name""#,
    );
    // Should have a Concat instruction
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::Concat(_, _, _))),
        "expected Concat for string interpolation, got: {instrs:?}"
    );
}

// ── field access tests ─────────────────────────────────────────────

#[test]
fn test_compile_field_access() {
    let instrs = compile("let obj = null\nobj.field");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::GetField(_, _, h) if *h == string_hash("field"))),
        "expected GetField for 'field', got: {instrs:?}"
    );
}

#[test]
fn test_compile_field_assign() {
    let instrs = compile("let obj = null\nobj.field = 42");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::SetField(_, h, _) if *h == string_hash("field"))),
        "expected SetField for 'field', got: {instrs:?}"
    );
}

// ── null coalesce tests ────────────────────────────────────────────

#[test]
fn test_compile_null_coalesce() {
    let instrs = compile("let x = null\nx ?? 0");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::NullCoalesce(_, _, _))),
        "expected NullCoalesce, got: {instrs:?}"
    );
}

// ── short-circuit And/Or tests ─────────────────────────────────────

#[test]
fn test_compile_short_circuit_and() {
    let instrs = compile_expr("true && false");
    // Short-circuit and: LoadBool(true), JumpIfFalsy (skip second operand)
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::JumpIfFalsy(_, _))),
        "expected JumpIfFalsy for short-circuit &&, got: {instrs:?}"
    );
}

#[test]
fn test_compile_short_circuit_or() {
    let instrs = compile_expr("true || false");
    // Short-circuit or: LoadBool(true), JumpIfTruthy (skip second operand)
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::JumpIfTruthy(_, _))),
        "expected JumpIfTruthy for short-circuit ||, got: {instrs:?}"
    );
}

// ── break/continue tests ───────────────────────────────────────────

#[test]
fn test_compile_break_in_while() {
    let instrs = compile("var x = 0\nwhile true { break }");
    // break emits a forward Jump
    let has_forward_jump = instrs
        .iter()
        .any(|i| matches!(i, Instruction::Jump(o) if *o >= 0));
    assert!(
        has_forward_jump,
        "break should emit a forward Jump, got: {instrs:?}"
    );
}

#[test]
fn test_compile_continue_in_while() {
    let instrs = compile("var x = 0\nwhile x < 10 { x += 1\ncontinue }");
    // Should have at least 2 backward jumps: loop's normal backward jump + continue
    let backward_jumps: Vec<_> = instrs
        .iter()
        .filter(|i| matches!(i, Instruction::Jump(o) if *o < 0))
        .collect();
    assert!(
        backward_jumps.len() >= 2,
        "expected at least 2 backward jumps (loop + continue), got {}",
        backward_jumps.len()
    );
}

#[test]
fn test_compile_break_outside_loop_error() {
    let err = compile_error("break");
    assert!(
        err.message.contains("outside of loop"),
        "expected 'break outside of loop' error, got: {}",
        err.message
    );
}

#[test]
fn test_compile_continue_outside_loop_error() {
    let err = compile_error("continue");
    assert!(
        err.message.contains("outside of loop"),
        "expected 'continue outside of loop' error, got: {}",
        err.message
    );
}

// ── Coroutine compilation tests ────────────────────────────────────

#[test]
fn test_compile_start_coroutine() {
    let instrs = compile("start foo()");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::StartCoroutine(_, 0))),
        "expected StartCoroutine(_, 0), got: {instrs:?}"
    );
}

#[test]
fn test_compile_start_coroutine_with_args() {
    let instrs = compile("start patrol(3)");
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::StartCoroutine(_, 1))),
        "expected StartCoroutine(_, 1), got: {instrs:?}"
    );
}

#[test]
fn test_compile_bare_yield() {
    let compiler = compile_full(
        r#"
        func coro() {
            yield
        }
    "#,
    );
    let funcs = compiler.functions();
    assert_eq!(funcs.len(), 1);
    let instrs = funcs[0].chunk.instructions();
    assert!(
        instrs.iter().any(|i| matches!(i, Instruction::Yield)),
        "expected Yield instruction, got: {instrs:?}"
    );
}

#[test]
fn test_compile_yield_seconds() {
    let compiler = compile_full(
        r#"
        func coro() {
            yield waitForSeconds(2.0)
        }
    "#,
    );
    let funcs = compiler.functions();
    assert_eq!(funcs.len(), 1);
    let instrs = funcs[0].chunk.instructions();
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadFloat(_, _))),
        "expected LoadFloat for seconds arg, got: {instrs:?}"
    );
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::YieldSeconds(_))),
        "expected YieldSeconds instruction, got: {instrs:?}"
    );
}

#[test]
fn test_compile_yield_frames() {
    let compiler = compile_full(
        r#"
        func coro() {
            yield waitForFrames(3)
        }
    "#,
    );
    let funcs = compiler.functions();
    assert_eq!(funcs.len(), 1);
    let instrs = funcs[0].chunk.instructions();
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::LoadInt(_, 3))),
        "expected LoadInt(_, 3) for frames arg, got: {instrs:?}"
    );
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::YieldFrames(_))),
        "expected YieldFrames instruction, got: {instrs:?}"
    );
}

#[test]
fn test_compile_yield_until() {
    let compiler = compile_full(
        r#"
        func coro() {
            yield waitUntil(isReady)
        }
    "#,
    );
    let funcs = compiler.functions();
    assert_eq!(funcs.len(), 1);
    let instrs = funcs[0].chunk.instructions();
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::YieldUntil(_))),
        "expected YieldUntil instruction, got: {instrs:?}"
    );
}

#[test]
fn test_compile_yield_coroutine() {
    let compiler = compile_full(
        r#"
        func child() {
            yield
        }
        func parent() {
            yield child()
        }
    "#,
    );
    let funcs = compiler.functions();
    assert_eq!(funcs.len(), 2);
    // parent is the second function
    let parent_instrs = funcs[1].chunk.instructions();
    assert!(
        parent_instrs
            .iter()
            .any(|i| matches!(i, Instruction::StartCoroutine(_, 0))),
        "expected StartCoroutine(_, 0) for child call, got: {parent_instrs:?}"
    );
    assert!(
        parent_instrs
            .iter()
            .any(|i| matches!(i, Instruction::YieldCoroutine(_, _))),
        "expected YieldCoroutine(_, _) after StartCoroutine, got: {parent_instrs:?}"
    );
}

#[test]
fn test_is_coroutine_flag_set() {
    let compiler = compile_full(
        r#"
        func coro() {
            yield
        }
    "#,
    );
    let funcs = compiler.functions();
    assert_eq!(funcs.len(), 1);
    assert!(
        funcs[0].is_coroutine,
        "function with yield should be marked as coroutine"
    );
}

#[test]
fn test_is_coroutine_flag_unset() {
    let compiler = compile_full(
        r#"
        func regular() {
            return 42
        }
    "#,
    );
    let funcs = compiler.functions();
    assert_eq!(funcs.len(), 1);
    assert!(
        !funcs[0].is_coroutine,
        "function without yield should not be marked as coroutine"
    );
}
