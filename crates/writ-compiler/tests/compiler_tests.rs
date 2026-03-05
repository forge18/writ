use writ_compiler::{Chunk, CompileError, Compiler, Instruction, string_hash};
use writ_lexer::Lexer;
use writ_parser::Parser;

// ── Test helpers ────────────────────────────────────────────────────

/// Compiles a single expression and returns the chunk.
fn compile_expr(source: &str) -> Chunk {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_expr().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_expr(&expr).expect("compile_expr failed");
    compiler.into_chunk()
}

/// Compiles a program (sequence of statements) and returns the chunk.
fn compile(source: &str) -> Chunk {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    compiler.into_chunk()
}

/// Compiles a program and returns the full Compiler (for accessing functions).
fn compile_full(source: &str) -> Compiler {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler.compile_program(&stmts).expect("compile failed");
    compiler
}

/// Compiles a program and expects a CompileError.
fn compile_error(source: &str) -> CompileError {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize().expect("lexer failed");
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse_program().expect("parser failed");
    let mut compiler = Compiler::new();
    compiler
        .compile_program(&stmts)
        .expect_err("expected CompileError")
}

// ── Phase 9 literal tests ──────────────────────────────────────────

#[test]
fn test_emit_int_literal() {
    let chunk = compile_expr("42");
    assert_eq!(chunk.instructions(), &[Instruction::LoadInt(42)]);
}

#[test]
fn test_emit_float_literal() {
    let chunk = compile_expr("3.14");
    assert_eq!(chunk.instructions(), &[Instruction::LoadFloat(3.14_f32)]);
}

#[test]
fn test_emit_bool_literal() {
    let chunk = compile_expr("true");
    assert_eq!(chunk.instructions(), &[Instruction::LoadBool(true)]);

    let chunk = compile_expr("false");
    assert_eq!(chunk.instructions(), &[Instruction::LoadBool(false)]);
}

#[test]
fn test_emit_string_literal() {
    let chunk = compile_expr(r#""hello""#);
    assert_eq!(chunk.instructions(), &[Instruction::LoadStr(0)]);
    assert_eq!(chunk.strings(), &["hello"]);
}

#[test]
fn test_emit_null_literal() {
    let chunk = compile_expr("null");
    assert_eq!(chunk.instructions(), &[Instruction::LoadNull]);
}

// ── Phase 9 arithmetic / operator tests ────────────────────────────

#[test]
fn test_emit_add() {
    // 1 + 2 is constant-folded to 3
    let chunk = compile_expr("1 + 2");
    assert_eq!(chunk.instructions(), &[Instruction::LoadInt(3)]);
}

#[test]
fn test_emit_operator_precedence() {
    // 1 + 2 * 3 should parse as 1 + (2 * 3)
    // 2 * 3 is folded to 6 (both literal), but 1 + (folded 6) can't fold further
    let chunk = compile_expr("1 + 2 * 3");
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadInt(1),
            Instruction::LoadInt(6),
            Instruction::AddInt,
        ]
    );
}

#[test]
fn test_emit_unary_negate() {
    // -42 is constant-folded to LoadInt(-42)
    let chunk = compile_expr("-42");
    assert_eq!(chunk.instructions(), &[Instruction::LoadInt(-42)]);
}

#[test]
fn test_emit_unary_not() {
    // !true is constant-folded to LoadBool(false)
    let chunk = compile_expr("!true");
    assert_eq!(chunk.instructions(), &[Instruction::LoadBool(false)]);
}

#[test]
fn test_emit_comparison() {
    // 1 < 2 is constant-folded to true
    let chunk = compile_expr("1 < 2");
    assert_eq!(chunk.instructions(), &[Instruction::LoadBool(true)]);
}

#[test]
fn test_emit_all_binary_ops() {
    // Literal-on-literal ops are now constant-folded.
    // Verify the folded results.
    let cases: &[(&str, &[Instruction])] = &[
        ("1 + 2", &[Instruction::LoadInt(3)]),
        ("1 - 2", &[Instruction::LoadInt(-1)]),
        ("1 * 2", &[Instruction::LoadInt(2)]),
        ("1 / 2", &[Instruction::LoadInt(0)]), // integer division
        ("1 % 2", &[Instruction::LoadInt(1)]),
        ("1 == 2", &[Instruction::LoadBool(false)]),
        ("1 != 2", &[Instruction::LoadBool(true)]),
        ("1 < 2", &[Instruction::LoadBool(true)]),
        ("1 > 2", &[Instruction::LoadBool(false)]),
        ("1 <= 2", &[Instruction::LoadBool(true)]),
        ("1 >= 2", &[Instruction::LoadBool(false)]),
    ];
    for (source, expected) in cases {
        let chunk = compile_expr(source);
        assert_eq!(chunk.instructions(), *expected, "failed for: {source}");
    }
}

// ── Phase 9 variable declaration tests ─────────────────────────────

#[test]
fn test_emit_let_decl() {
    let chunk = compile("let x = 42");
    assert_eq!(
        chunk.instructions(),
        &[Instruction::LoadInt(42), Instruction::StoreLocal(0)]
    );
}

#[test]
fn test_emit_var_load() {
    let chunk = compile("let x = 42\nx");
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadInt(42),
            Instruction::StoreLocal(0),
            Instruction::LoadLocal(0),
            Instruction::Pop,
        ]
    );
}

#[test]
fn test_emit_multiple_locals() {
    let chunk = compile("let x = 1\nlet y = 2\nlet z = 3");
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadInt(1),
            Instruction::StoreLocal(0),
            Instruction::LoadInt(2),
            Instruction::StoreLocal(1),
            Instruction::LoadInt(3),
            Instruction::StoreLocal(2),
        ]
    );
}

// ── Phase 9 assignment tests ───────────────────────────────────────

#[test]
fn test_emit_assignment() {
    let chunk = compile("var x = 0\nx = 5");
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadInt(0),
            Instruction::StoreLocal(0),
            Instruction::LoadInt(5),
            Instruction::StoreLocal(0),
        ]
    );
}

#[test]
fn test_emit_compound_assignment() {
    // x += 1 is directly emitted as IncrLocalInt(0, 1)
    let chunk = compile("var x = 10\nx += 1");
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadInt(10),
            Instruction::StoreLocal(0),
            Instruction::IncrLocalInt(0, 1),
        ]
    );
}

#[test]
fn test_emit_all_compound_ops() {
    // += and -= on int literals use IncrLocalInt; others use generic path
    let cases: &[(&str, &[Instruction])] = &[
        (
            "x += 1",
            &[
                Instruction::LoadInt(10),
                Instruction::StoreLocal(0),
                Instruction::IncrLocalInt(0, 1),
            ],
        ),
        (
            "x -= 1",
            &[
                Instruction::LoadInt(10),
                Instruction::StoreLocal(0),
                Instruction::IncrLocalInt(0, -1),
            ],
        ),
        (
            "x *= 1",
            &[
                Instruction::LoadInt(10),
                Instruction::StoreLocal(0),
                Instruction::LoadLocal(0),
                Instruction::LoadInt(1),
                Instruction::Mul,
                Instruction::StoreLocal(0),
            ],
        ),
        (
            "x /= 1",
            &[
                Instruction::LoadInt(10),
                Instruction::StoreLocal(0),
                Instruction::LoadLocal(0),
                Instruction::LoadInt(1),
                Instruction::Div,
                Instruction::StoreLocal(0),
            ],
        ),
        (
            "x %= 1",
            &[
                Instruction::LoadInt(10),
                Instruction::StoreLocal(0),
                Instruction::LoadLocal(0),
                Instruction::LoadInt(1),
                Instruction::Mod,
                Instruction::StoreLocal(0),
            ],
        ),
    ];
    for (assign_source, expected) in cases {
        let source = format!("var x = 10\n{assign_source}");
        let chunk = compile(&source);
        assert_eq!(
            chunk.instructions(),
            *expected,
            "failed for: {assign_source}"
        );
    }
}

// ── Phase 9 expression statement tests ─────────────────────────────

#[test]
fn test_expr_stmt_pops() {
    let chunk = compile("42");
    assert_eq!(
        chunk.instructions(),
        &[Instruction::LoadInt(42), Instruction::Pop]
    );
}

// ── Phase 9 string deduplication test ──────────────────────────────

#[test]
fn test_string_deduplication() {
    let chunk = compile(
        r#"let x = "hello"
let y = "hello""#,
    );
    // Both use the same constant pool index
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadStr(0),
            Instruction::StoreLocal(0),
            Instruction::LoadStr(0),
            Instruction::StoreLocal(1),
        ]
    );
    assert_eq!(chunk.strings(), &["hello"]);
}

// ── Phase 9 line number tracking test ──────────────────────────────

#[test]
fn test_line_numbers() {
    let chunk = compile("let x = 1\nlet y = 2");
    // Line 1: LoadInt(1), StoreLocal(0)
    // Line 2: LoadInt(2), StoreLocal(1)
    assert_eq!(chunk.line(0), 1); // LoadInt(1)
    assert_eq!(chunk.line(1), 1); // StoreLocal(0)
    assert_eq!(chunk.line(2), 2); // LoadInt(2)
    assert_eq!(chunk.line(3), 2); // StoreLocal(1)
}

// ── Phase 9 error tests ───────────────────────────────────────────

#[test]
fn test_large_int_emits_load_big_int() {
    // Integers that don't fit i32 should emit LoadConstInt via constant pool.
    let chunk = compile("let x = 9999999999999");
    assert!(
        chunk
            .instructions()
            .iter()
            .any(|inst| matches!(inst, Instruction::LoadConstInt(0))),
        "expected LoadConstInt(0), got: {:?}",
        chunk.instructions()
    );
    assert_eq!(chunk.int64_constants(), &[9999999999999i64]);
}

// Note: In Phase 10, undefined variables resolve as function name strings
// (LoadStr) instead of producing a compile error. This is by design for
// runtime function resolution. The test_undefined_variable_error from
// Phase 9 is replaced by the new identifier resolution behavior.

// ══════════════════════════════════════════════════════════════════════
// Phase 10 tests
// ══════════════════════════════════════════════════════════════════════

// ── if/else tests ──────────────────────────────────────────────────

#[test]
fn test_compile_if_true_branch() {
    // if true { let x = 1 }
    let chunk = compile("if true { let x = 1 }");
    let instrs = chunk.instructions();
    // LoadBool(true), JumpIfFalsePop(?), LoadInt(1), StoreLocal(0), Pop (scope cleanup)
    assert_eq!(instrs[0], Instruction::LoadBool(true));
    assert!(matches!(instrs[1], Instruction::JumpIfFalsePop(_)));
    assert_eq!(instrs[2], Instruction::LoadInt(1));
    assert_eq!(instrs[3], Instruction::StoreLocal(0));
    assert_eq!(instrs[4], Instruction::Pop); // scope cleanup
}

#[test]
fn test_compile_if_false_branch() {
    // Same structure as if_true_branch — the VM determines which path
    let chunk = compile("if false { let x = 1 }");
    let instrs = chunk.instructions();
    assert_eq!(instrs[0], Instruction::LoadBool(false));
    assert!(matches!(instrs[1], Instruction::JumpIfFalsePop(_)));
}

#[test]
fn test_compile_if_else() {
    let chunk = compile("let x = true\nif x { let a = 1 } else { let b = 2 }");
    let instrs = chunk.instructions();
    // let x = true: LoadBool, StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadBool(true));
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    // if x: LoadLocal(0), JumpIfFalsePop
    assert_eq!(instrs[2], Instruction::LoadLocal(0));
    assert!(matches!(instrs[3], Instruction::JumpIfFalsePop(_)));
    // then block: LoadInt(1), StoreLocal(1), Pop (scope cleanup)
    assert_eq!(instrs[4], Instruction::LoadInt(1));
    assert_eq!(instrs[5], Instruction::StoreLocal(1));
    assert_eq!(instrs[6], Instruction::Pop); // scope cleanup
    // Jump over else
    assert!(matches!(instrs[7], Instruction::Jump(_)));
    // else block: LoadInt(2), StoreLocal(1), Pop (scope cleanup)
    assert_eq!(instrs[8], Instruction::LoadInt(2));
    assert_eq!(instrs[9], Instruction::StoreLocal(1));
    assert_eq!(instrs[10], Instruction::Pop); // scope cleanup
}

// ── while loop tests ───────────────────────────────────────────────

#[test]
fn test_compile_while() {
    let chunk = compile("var x = 0\nwhile x < 10 { x += 1 }");
    let instrs = chunk.instructions();
    // var x = 0: LoadInt(0), StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadInt(0));
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    // condition: LoadLocal(0), LoadInt(10), LtInt (typed — x is known int)
    assert_eq!(instrs[2], Instruction::LoadLocal(0));
    assert_eq!(instrs[3], Instruction::LoadInt(10));
    assert_eq!(instrs[4], Instruction::LtInt);
    // JumpIfFalsePop to exit
    assert!(matches!(instrs[5], Instruction::JumpIfFalsePop(_)));
    // body: x += 1 → IncrLocalInt(0, 1)
    assert_eq!(instrs[6], Instruction::IncrLocalInt(0, 1));
    // backward jump
    assert!(matches!(instrs[7], Instruction::Jump(_)));
    if let Instruction::Jump(offset) = instrs[7] {
        assert!(offset < 0, "backward jump should have negative offset");
    }
}

// ── for loop tests ─────────────────────────────────────────────────

#[test]
fn test_compile_for_range() {
    let chunk = compile("for i in 0..3 { i }");
    let instrs = chunk.instructions();
    // Setup: store start (0) in __iter, store end (3) in __end
    assert_eq!(instrs[0], Instruction::LoadInt(0));
    assert_eq!(instrs[1], Instruction::StoreLocal(0)); // __iter
    assert_eq!(instrs[2], Instruction::LoadInt(3));
    assert_eq!(instrs[3], Instruction::StoreLocal(1)); // __end
    // Condition: LoadLocal(__iter), LoadLocal(__end), Lt
    assert_eq!(instrs[4], Instruction::LoadLocal(0));
    assert_eq!(instrs[5], Instruction::LoadLocal(1));
    assert_eq!(instrs[6], Instruction::Lt);
    // JumpIfFalsePop to exit
    assert!(matches!(instrs[7], Instruction::JumpIfFalsePop(_)));
    // Bind i = __iter
    assert_eq!(instrs[8], Instruction::LoadLocal(0));
    assert_eq!(instrs[9], Instruction::StoreLocal(2)); // i
}

#[test]
fn test_compile_for_in_array() {
    let chunk = compile("let arr = [1, 2, 3]\nfor item in arr { item }");
    let instrs = chunk.instructions();
    // let arr = [1,2,3]: LoadInt(1), LoadInt(2), LoadInt(3), MakeArray(3), StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadInt(1));
    assert_eq!(instrs[1], Instruction::LoadInt(2));
    assert_eq!(instrs[2], Instruction::LoadInt(3));
    assert_eq!(instrs[3], Instruction::MakeArray(3));
    assert_eq!(instrs[4], Instruction::StoreLocal(0)); // arr

    // For-in-array setup: store arr as __arr, get length, init counter
    assert_eq!(instrs[5], Instruction::LoadLocal(0)); // load arr
    assert_eq!(instrs[6], Instruction::StoreLocal(1)); // __arr
    assert_eq!(instrs[7], Instruction::LoadLocal(1)); // load __arr
    assert_eq!(instrs[8], Instruction::GetField(string_hash("length")));
    assert_eq!(instrs[9], Instruction::StoreLocal(2)); // __len
    assert_eq!(instrs[10], Instruction::LoadInt(0));
    assert_eq!(instrs[11], Instruction::StoreLocal(3)); // __idx
}

// ── when statement tests ───────────────────────────────────────────

#[test]
fn test_compile_when_value() {
    let chunk = compile("let x = 1\nwhen x { 0 => 42 }");
    let instrs = chunk.instructions();
    // let x = 1: LoadInt(1), StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadInt(1));
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    // when: store subject as __subject
    assert_eq!(instrs[2], Instruction::LoadLocal(0));
    assert_eq!(instrs[3], Instruction::StoreLocal(1)); // __subject
    // arm: LoadLocal(__subject), LoadInt(0), Eq, JumpIfFalsePop
    assert_eq!(instrs[4], Instruction::LoadLocal(1));
    assert_eq!(instrs[5], Instruction::LoadInt(0));
    assert_eq!(instrs[6], Instruction::Eq);
    assert!(matches!(instrs[7], Instruction::JumpIfFalsePop(_)));
}

#[test]
fn test_compile_when_else() {
    let chunk = compile("let x = 1\nwhen x { 0 => 10; else => 20 }");
    let instrs = chunk.instructions();
    // Setup: let x, store __subject
    assert_eq!(instrs[0], Instruction::LoadInt(1));
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    assert_eq!(instrs[2], Instruction::LoadLocal(0));
    assert_eq!(instrs[3], Instruction::StoreLocal(1));
    // First arm (0): comparison + conditional jump
    assert_eq!(instrs[4], Instruction::LoadLocal(1));
    assert_eq!(instrs[5], Instruction::LoadInt(0));
    assert_eq!(instrs[6], Instruction::Eq);
    assert!(matches!(instrs[7], Instruction::JumpIfFalsePop(_)));
    // Verify else arm exists by finding LoadInt(20) somewhere later
    let has_else_body = instrs.iter().any(|i| matches!(i, Instruction::LoadInt(20)));
    assert!(has_else_body, "else arm body should contain LoadInt(20)");
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
    // instead of generic Add. No peephole in this test helper.
    let func_instrs = functions[0].chunk.instructions();
    assert_eq!(func_instrs[0], Instruction::LoadLocal(0)); // a
    assert_eq!(func_instrs[1], Instruction::LoadLocal(1)); // b
    assert_eq!(func_instrs[2], Instruction::AddInt);
    assert_eq!(func_instrs[3], Instruction::Return);
}

#[test]
fn test_compile_function_call() {
    let chunk = compile("func foo() { }\nfoo()");
    let instrs = chunk.instructions();
    // foo() is a known compiled function → CallDirect(0, 0)
    let has_call = instrs
        .iter()
        .any(|i| matches!(i, Instruction::CallDirect(0, 0)));
    assert!(has_call, "should emit CallDirect(0, 0) for foo()");
}

#[test]
fn test_compile_return_value() {
    let compiler = compile_full("func get() -> int { return 42 }");
    let functions = compiler.functions();
    assert_eq!(functions.len(), 1);
    let func_instrs = functions[0].chunk.instructions();
    assert_eq!(func_instrs[0], Instruction::LoadInt(42));
    assert_eq!(func_instrs[1], Instruction::Return);
}

#[test]
fn test_compile_function_implicit_return() {
    let compiler = compile_full("func noop() { }");
    let functions = compiler.functions();
    assert_eq!(functions.len(), 1);
    let func_instrs = functions[0].chunk.instructions();
    // Should have implicit LoadNull + Return
    assert_eq!(func_instrs[0], Instruction::LoadNull);
    assert_eq!(func_instrs[1], Instruction::Return);
}

// ── collection literal tests ───────────────────────────────────────

#[test]
fn test_compile_array_literal() {
    let chunk = compile_expr("[1, 2, 3]");
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadInt(1),
            Instruction::LoadInt(2),
            Instruction::LoadInt(3),
            Instruction::MakeArray(3),
        ]
    );
}

#[test]
fn test_compile_dict_literal() {
    let chunk = compile_expr(r#"{"a": 1, "b": 2}"#);
    assert_eq!(
        chunk.instructions(),
        &[
            Instruction::LoadStr(0), // "a"
            Instruction::LoadInt(1),
            Instruction::LoadStr(1), // "b"
            Instruction::LoadInt(2),
            Instruction::MakeDict(2),
        ]
    );
    assert_eq!(chunk.strings(), &["a", "b"]);
}

// ── string interpolation tests ─────────────────────────────────────

#[test]
fn test_compile_string_concat() {
    // "hello $name" where name is a local
    let chunk = compile(
        r#"let name = "world"
"hello $name""#,
    );
    let instrs = chunk.instructions();
    // let name = "world": LoadStr("world"), StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadStr(0));
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    // String interpolation: LoadStr("hello "), LoadLocal(0), Concat, Pop
    assert_eq!(instrs[2], Instruction::LoadStr(1)); // "hello "
    assert_eq!(instrs[3], Instruction::LoadLocal(0)); // name
    assert_eq!(instrs[4], Instruction::Concat);
    assert_eq!(instrs[5], Instruction::Pop); // expr stmt pop
}

// ── field access tests ─────────────────────────────────────────────

#[test]
fn test_compile_field_access() {
    let chunk = compile("let obj = null\nobj.field");
    let instrs = chunk.instructions();
    // let obj = null: LoadNull, StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadNull);
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    // obj.field: LoadLocal(0), GetField(hash("field")), Pop
    assert_eq!(instrs[2], Instruction::LoadLocal(0));
    assert_eq!(instrs[3], Instruction::GetField(string_hash("field")));
    assert_eq!(instrs[4], Instruction::Pop);
}

#[test]
fn test_compile_field_assign() {
    let chunk = compile("let obj = null\nobj.field = 42");
    let instrs = chunk.instructions();
    // let obj = null: LoadNull, StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadNull);
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    // obj.field = 42: LoadLocal(0), LoadInt(42), SetField(hash("field"))
    assert_eq!(instrs[2], Instruction::LoadLocal(0));
    assert_eq!(instrs[3], Instruction::LoadInt(42));
    assert_eq!(instrs[4], Instruction::SetField(string_hash("field")));
}

// ── null coalesce tests ────────────────────────────────────────────

#[test]
fn test_compile_null_coalesce() {
    let chunk = compile("let x = null\nx ?? 0");
    let instrs = chunk.instructions();
    // let x = null: LoadNull, StoreLocal(0)
    assert_eq!(instrs[0], Instruction::LoadNull);
    assert_eq!(instrs[1], Instruction::StoreLocal(0));
    // x ?? 0: LoadLocal(0), LoadInt(0), NullCoalesce, Pop
    assert_eq!(instrs[2], Instruction::LoadLocal(0));
    assert_eq!(instrs[3], Instruction::LoadInt(0));
    assert_eq!(instrs[4], Instruction::NullCoalesce);
    assert_eq!(instrs[5], Instruction::Pop);
}

// ── short-circuit And/Or tests ─────────────────────────────────────

#[test]
fn test_compile_short_circuit_and() {
    let chunk = compile_expr("true && false");
    let instrs = chunk.instructions();
    // true && false: LoadBool(true), JumpIfFalse(end), Pop, LoadBool(false)
    assert_eq!(instrs[0], Instruction::LoadBool(true));
    assert!(matches!(instrs[1], Instruction::JumpIfFalse(_)));
    assert_eq!(instrs[2], Instruction::Pop);
    assert_eq!(instrs[3], Instruction::LoadBool(false));
}

#[test]
fn test_compile_short_circuit_or() {
    let chunk = compile_expr("true || false");
    let instrs = chunk.instructions();
    // true || false: LoadBool(true), JumpIfTrue(end), Pop, LoadBool(false)
    assert_eq!(instrs[0], Instruction::LoadBool(true));
    assert!(matches!(instrs[1], Instruction::JumpIfTrue(_)));
    assert_eq!(instrs[2], Instruction::Pop);
    assert_eq!(instrs[3], Instruction::LoadBool(false));
}

// ── break/continue tests ───────────────────────────────────────────

#[test]
fn test_compile_break_in_while() {
    let chunk = compile("var x = 0\nwhile true { break }");
    let instrs = chunk.instructions();
    // Find the break jump (a Jump(0) placeholder that gets patched)
    let has_forward_jump = instrs.iter().any(|i| match i {
        Instruction::Jump(offset) => *offset >= 0,
        _ => false,
    });
    assert!(
        has_forward_jump,
        "break should emit a forward Jump after the loop"
    );
}

#[test]
fn test_compile_continue_in_while() {
    let chunk = compile("var x = 0\nwhile x < 10 { x += 1\ncontinue }");
    let instrs = chunk.instructions();
    // Continue should emit a backward Jump (negative offset)
    let backward_jumps: Vec<_> = instrs
        .iter()
        .filter(|i| matches!(i, Instruction::Jump(o) if *o < 0))
        .collect();
    // Should have at least 2 backward jumps: the loop's normal backward jump + continue
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

// ── Phase 13: Coroutine compilation tests ───────────────────────────

#[test]
fn test_compile_start_coroutine() {
    let chunk = compile("start foo()");
    let instrs = chunk.instructions();
    // Should emit: LoadStr("foo"), StartCoroutine(0), Pop
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::StartCoroutine(0))),
        "expected StartCoroutine(0), got: {instrs:?}"
    );
    // Pop to discard the coroutine handle
    assert!(
        instrs.iter().any(|i| matches!(i, Instruction::Pop)),
        "expected Pop after StartCoroutine, got: {instrs:?}"
    );
}

#[test]
fn test_compile_start_coroutine_with_args() {
    let chunk = compile("start patrol(3)");
    let instrs = chunk.instructions();
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::StartCoroutine(1))),
        "expected StartCoroutine(1), got: {instrs:?}"
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
            .any(|i| matches!(i, Instruction::LoadFloat(_))),
        "expected LoadFloat for seconds arg, got: {instrs:?}"
    );
    assert!(
        instrs
            .iter()
            .any(|i| matches!(i, Instruction::YieldSeconds)),
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
        instrs.iter().any(|i| matches!(i, Instruction::LoadInt(3))),
        "expected LoadInt(3) for frames arg, got: {instrs:?}"
    );
    assert!(
        instrs.iter().any(|i| matches!(i, Instruction::YieldFrames)),
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
        instrs.iter().any(|i| matches!(i, Instruction::YieldUntil)),
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
            .any(|i| matches!(i, Instruction::StartCoroutine(0))),
        "expected StartCoroutine(0) for child call, got: {parent_instrs:?}"
    );
    assert!(
        parent_instrs
            .iter()
            .any(|i| matches!(i, Instruction::YieldCoroutine)),
        "expected YieldCoroutine after StartCoroutine, got: {parent_instrs:?}"
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
