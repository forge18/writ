use criterion::{Criterion, criterion_group, criterion_main};
use writ::Writ;

// ── Cross-language benchmark programs ────────────────────────────────
// End-to-end pipeline: lex → parse → compile → execute

/// Recursive fibonacci — standard function call overhead benchmark.
/// Used by: Wren, AWFY, CLBG, Rhai
const FIBONACCI: &str = "\
func fib(n: int) -> int {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
return fib(28)";

/// Binary trees — object allocation and recursive traversal.
/// Used by: Wren, CLBG, AWFY
const BINARY_TREES: &str = "\
class Tree {
    public item: int
    public left: int
    public right: int
}
func make(item: int, depth: int) -> int {
    if depth == 0 { return Tree(item, null, null) }
    let item2 = item + item
    let d = depth - 1
    return Tree(item, make(item2 - 1, d), make(item2, d))
}
func check(node: int) -> int {
    if node.left == null { return node.item }
    return node.item + check(node.left) - check(node.right)
}
var sum = 0
var i = 1
while i <= 100 {
    sum += check(make(i, 8))
    sum += check(make(0 - i, 8))
    i += 1
}
return sum";

/// Permute — recursion and counting (Heap's algorithm structure).
/// Used by: AWFY
const PERMUTE: &str = "\
func permute(n: int) -> int {
    if n <= 1 { return 1 }
    var count = 0
    var i = 0
    while i < n {
        count += permute(n - 1)
        i += 1
    }
    return count
}
return permute(9)";

/// Mandelbrot — floating point arithmetic and nested loops.
/// Used by: AWFY, CLBG
const MANDELBROT: &str = "\
func mandelbrot(size: int) -> int {
    var sum = 0
    let fsize = size * 1.0
    var y = 0
    while y < size {
        var x = 0
        while x < size {
            let cr = 2.0 * (x * 1.0) / fsize - 1.5
            let ci = 2.0 * (y * 1.0) / fsize - 1.0
            var zr = 0.0
            var zi = 0.0
            var iter = 0
            var inside = true
            while iter < 50 {
                let tr = zr * zr - zi * zi + cr
                zi = 2.0 * zr * zi + ci
                zr = tr
                if zr * zr + zi * zi > 4.0 {
                    inside = false
                    iter = 50
                }
                iter += 1
            }
            if inside { sum += 1 }
            x += 1
        }
        y += 1
    }
    return sum
}
return mandelbrot(100)";

/// Sieve of Eratosthenes — array subscript and index assignment.
/// Used by: Wren, AWFY, CLBG
const SIEVE: &str = "\
func sieve(size: int) -> int {
    var flags = [false]
    var i = 1
    while i < size {
        flags.push(true)
        i += 1
    }
    var count = 0
    i = 2
    while i < size {
        if flags[i] {
            count += 1
            var j = i + i
            while j < size {
                flags[j] = false
                j += i
            }
        }
        i += 1
    }
    return count
}
return sieve(5000)";

/// N-Queens solver — exercises closures and nested function capture.
/// Used by: AWFY, Wren
const QUEENS: &str = "\
func queens(n: int) -> int {
    var solutions = 0
    var cols = [0]
    var i = 1
    while i < n {
        cols.push(0)
        i += 1
    }
    func valid(row: int, col: int) -> bool {
        var r = 0
        while r < row {
            let c = cols[r]
            if c == col { return false }
            if c - r == col - row { return false }
            if c + r == col + row { return false }
            r += 1
        }
        return true
    }
    func solve(row: int) -> int {
        if row == n {
            solutions += 1
            return 0
        }
        var c = 0
        while c < n {
            if valid(row, c) {
                cols[row] = c
                solve(row + 1)
            }
            c += 1
        }
        return 0
    }
    solve(0)
    return solutions
}
return queens(8)";

/// Tight loop — instruction dispatch throughput baseline.
const LOOP_SUM: &str = "\
var sum = 0
var i = 0
while i < 10000 {
    sum += i
    i += 1
}
return sum";

fn bench_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline");

    group.bench_function("fibonacci_28", |b| {
        b.iter(|| {
            let mut writ = Writ::new();
            writ.disable_type_checking();
            writ.run(FIBONACCI).unwrap()
        });
    });

    group.bench_function("binary_trees", |b| {
        b.iter(|| {
            let mut writ = Writ::new();
            writ.disable_type_checking();
            writ.run(BINARY_TREES).unwrap()
        });
    });

    group.bench_function("permute_9", |b| {
        b.iter(|| {
            let mut writ = Writ::new();
            writ.disable_type_checking();
            writ.run(PERMUTE).unwrap()
        });
    });

    group.bench_function("mandelbrot_100", |b| {
        b.iter(|| {
            let mut writ = Writ::new();
            writ.disable_type_checking();
            writ.run(MANDELBROT).unwrap()
        });
    });

    group.bench_function("sieve_5000", |b| {
        b.iter(|| {
            let mut writ = Writ::new();
            writ.disable_type_checking();
            writ.run(SIEVE).unwrap()
        });
    });

    group.bench_function("queens_8", |b| {
        b.iter(|| {
            let mut writ = Writ::new();
            writ.disable_type_checking();
            writ.run(QUEENS).unwrap()
        });
    });

    group.bench_function("loop_sum", |b| {
        b.iter(|| {
            let mut writ = Writ::new();
            writ.disable_type_checking();
            writ.run(LOOP_SUM).unwrap()
        });
    });

    group.finish();
}

criterion_group!(benches, bench_pipeline);
criterion_main!(benches);
