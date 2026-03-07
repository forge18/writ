/// Runs a single benchmark for flamegraph profiling.
/// Usage: cargo flamegraph --example profile_bottlenecks --release -- <benchmark>
/// Available benchmarks: fibonacci, binary_trees, permute, mandelbrot, sieve, queens, loop_sum
use std::time::Instant;
use writ::compiler::{Chunk, ClassMeta, CompiledFunction, Compiler, StructMeta};
use writ::lexer::Lexer;
use writ::parser::Parser;
use writ::vm::VM;

const FIBONACCI: &str = "\
func fib(n: int) -> int {
    if n <= 1 { return n }
    return fib(n - 1) + fib(n - 2)
}
return fib(28)";

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

const LOOP_SUM: &str = "\
var sum = 0
var i = 0
while i < 10000 {
    sum += i
    i += 1
}
return sum";

fn compile(
    source: &str,
) -> (
    Chunk,
    Vec<CompiledFunction>,
    Vec<StructMeta>,
    Vec<ClassMeta>,
) {
    let tokens = Lexer::new(source).tokenize().unwrap();
    let stmts = Parser::new(tokens).parse_program().unwrap();
    let mut compiler = Compiler::new();
    for stmt in &stmts {
        compiler.compile_stmt(stmt).unwrap();
    }
    compiler.into_parts()
}

fn run_n(source: &str, n: usize) {
    let (chunk, fns, structs, classes) = compile(source);
    let start = Instant::now();
    for _ in 0..n {
        let mut vm = VM::new();
        writ::stdlib::register_all(&mut vm);
        std::hint::black_box(
            vm.execute_program(&chunk, &fns, &structs, &classes)
                .unwrap(),
        );
    }
    let elapsed = start.elapsed();
    eprintln!(
        "  {n} iterations in {elapsed:.3?} ({:.3?}/iter)",
        elapsed / n as u32
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bench = args.get(1).map(|s| s.as_str()).unwrap_or("all");

    // Run enough iterations to get ~2-3 seconds of samples for flamegraph
    match bench {
        "fibonacci" => run_n(FIBONACCI, 100),
        "binary_trees" => run_n(BINARY_TREES, 30),
        "permute" => run_n(PERMUTE, 100),
        "mandelbrot" => run_n(MANDELBROT, 100),
        "sieve" => run_n(SIEVE, 3000),
        "queens" => run_n(QUEENS, 400),
        "loop_sum" => run_n(LOOP_SUM, 10000),
        "all" => {
            eprintln!("fibonacci:");
            run_n(FIBONACCI, 100);
            eprintln!("binary_trees:");
            run_n(BINARY_TREES, 30);
            eprintln!("permute:");
            run_n(PERMUTE, 100);
            eprintln!("mandelbrot:");
            run_n(MANDELBROT, 100);
            eprintln!("sieve:");
            run_n(SIEVE, 3000);
            eprintln!("queens:");
            run_n(QUEENS, 400);
            eprintln!("loop_sum:");
            run_n(LOOP_SUM, 10000);
        }
        _ => {
            eprintln!("Unknown benchmark: {bench}");
            eprintln!(
                "Available: fibonacci, binary_trees, permute, mandelbrot, sieve, queens, loop_sum, all"
            );
            std::process::exit(1);
        }
    }
}
