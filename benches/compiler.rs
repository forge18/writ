use criterion::{Criterion, criterion_group, criterion_main};
use writ_compiler::Compiler;
use writ_lexer::Lexer;
use writ_parser::Parser;

const FIBONACCI: &str = "\
func fib(n: int) -> int {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
return fib(20)";

const STRUCTS: &str = "\
struct Point {
    public x: int
    public y: int

    func sum() -> int {
        return self.x + self.y
    }
}
let p = Point(3, 7)
return p.sum()";

const LOOP: &str = "\
var sum = 0
var i = 0
while i < 10000 {
    sum += i
    i += 1
}
return sum";

fn bench_compiler(c: &mut Criterion) {
    let mut group = c.benchmark_group("compiler");

    let fib_stmts = Parser::new(Lexer::new(FIBONACCI).tokenize().unwrap())
        .parse_program()
        .unwrap();
    group.bench_function("fibonacci", |b| {
        b.iter(|| {
            let mut compiler = Compiler::new();
            compiler.compile_program(&fib_stmts).unwrap();
            compiler.into_parts()
        });
    });

    let struct_stmts = Parser::new(Lexer::new(STRUCTS).tokenize().unwrap())
        .parse_program()
        .unwrap();
    group.bench_function("structs", |b| {
        b.iter(|| {
            let mut compiler = Compiler::new();
            compiler.compile_program(&struct_stmts).unwrap();
            compiler.into_parts()
        });
    });

    let loop_stmts = Parser::new(Lexer::new(LOOP).tokenize().unwrap())
        .parse_program()
        .unwrap();
    group.bench_function("loop", |b| {
        b.iter(|| {
            let mut compiler = Compiler::new();
            compiler.compile_program(&loop_stmts).unwrap();
            compiler.into_parts()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_compiler);
criterion_main!(benches);
