use criterion::{Criterion, criterion_group, criterion_main};
use writ::lexer::Lexer;
use writ::parser::Parser;

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

fn bench_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");

    let fib_tokens = Lexer::new(FIBONACCI).tokenize().unwrap();
    group.bench_function("fibonacci", |b| {
        b.iter(|| Parser::new(fib_tokens.clone()).parse_program().unwrap());
    });

    let struct_tokens = Lexer::new(STRUCTS).tokenize().unwrap();
    group.bench_function("structs", |b| {
        b.iter(|| Parser::new(struct_tokens.clone()).parse_program().unwrap());
    });

    let loop_tokens = Lexer::new(LOOP).tokenize().unwrap();
    group.bench_function("loop", |b| {
        b.iter(|| Parser::new(loop_tokens.clone()).parse_program().unwrap());
    });

    group.finish();
}

criterion_group!(benches, bench_parser);
criterion_main!(benches);
