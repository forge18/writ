use criterion::{Criterion, criterion_group, criterion_main};
use writ_lexer::Lexer;

const ARITHMETIC: &str = "return 1 + 2 * 3 - 4 / 5";

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

const INTERPOLATION: &str = r#"
let name = "world"
let x = 42
return "hello ${name}, value=${x}"
"#;

fn bench_lexer(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");

    group.bench_function("arithmetic", |b| {
        b.iter(|| Lexer::new(ARITHMETIC).tokenize().unwrap());
    });

    group.bench_function("fibonacci", |b| {
        b.iter(|| Lexer::new(FIBONACCI).tokenize().unwrap());
    });

    group.bench_function("structs", |b| {
        b.iter(|| Lexer::new(STRUCTS).tokenize().unwrap());
    });

    group.bench_function("interpolation", |b| {
        b.iter(|| Lexer::new(INTERPOLATION).tokenize().unwrap());
    });

    group.finish();
}

criterion_group!(benches, bench_lexer);
criterion_main!(benches);
