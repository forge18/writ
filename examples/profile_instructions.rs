/// Profiles instruction pair/triple frequency across all benchmark programs.
/// Run with: cargo run --example profile_instructions --release
use std::collections::HashMap;
use writ_compiler::{Compiler, Instruction};
use writ_lexer::Lexer;
use writ_parser::Parser;

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

fn opcode_name(instr: &Instruction) -> &'static str {
    match instr {
        Instruction::LoadInt(..) => "LoadInt",
        Instruction::LoadConstInt(..) => "LoadConstInt",
        Instruction::LoadFloat(..) => "LoadFloat",
        Instruction::LoadConstFloat(..) => "LoadConstFloat",
        Instruction::LoadBool(..) => "LoadBool",
        Instruction::LoadStr(..) => "LoadStr",
        Instruction::LoadNull(..) => "LoadNull",
        Instruction::Move(..) => "Move",
        Instruction::LoadGlobal(..) => "LoadGlobal",
        Instruction::Add(..) => "Add",
        Instruction::Sub(..) => "Sub",
        Instruction::Mul(..) => "Mul",
        Instruction::Div(..) => "Div",
        Instruction::Mod(..) => "Mod",
        Instruction::AddInt(..) => "AddInt",
        Instruction::AddFloat(..) => "AddFloat",
        Instruction::SubInt(..) => "SubInt",
        Instruction::SubFloat(..) => "SubFloat",
        Instruction::MulInt(..) => "MulInt",
        Instruction::MulFloat(..) => "MulFloat",
        Instruction::DivInt(..) => "DivInt",
        Instruction::DivFloat(..) => "DivFloat",
        Instruction::AddIntImm(..) => "AddIntImm",
        Instruction::SubIntImm(..) => "SubIntImm",
        Instruction::Neg(..) => "Neg",
        Instruction::Not(..) => "Not",
        Instruction::Eq(..) => "Eq",
        Instruction::Ne(..) => "Ne",
        Instruction::Lt(..) => "Lt",
        Instruction::Le(..) => "Le",
        Instruction::Gt(..) => "Gt",
        Instruction::Ge(..) => "Ge",
        Instruction::EqInt(..) => "EqInt",
        Instruction::EqFloat(..) => "EqFloat",
        Instruction::NeInt(..) => "NeInt",
        Instruction::NeFloat(..) => "NeFloat",
        Instruction::LtInt(..) => "LtInt",
        Instruction::LtFloat(..) => "LtFloat",
        Instruction::LeInt(..) => "LeInt",
        Instruction::LeFloat(..) => "LeFloat",
        Instruction::GtInt(..) => "GtInt",
        Instruction::GtFloat(..) => "GtFloat",
        Instruction::GeInt(..) => "GeInt",
        Instruction::GeFloat(..) => "GeFloat",
        Instruction::And(..) => "And",
        Instruction::Or(..) => "Or",
        Instruction::Jump(..) => "Jump",
        Instruction::JumpIfFalsy(..) => "JumpIfFalsy",
        Instruction::JumpIfTruthy(..) => "JumpIfTruthy",
        Instruction::TestLtInt(..) => "TestLtInt",
        Instruction::TestLeInt(..) => "TestLeInt",
        Instruction::TestGtInt(..) => "TestGtInt",
        Instruction::TestGeInt(..) => "TestGeInt",
        Instruction::TestEqInt(..) => "TestEqInt",
        Instruction::TestNeInt(..) => "TestNeInt",
        Instruction::TestLtIntImm(..) => "TestLtIntImm",
        Instruction::TestLeIntImm(..) => "TestLeIntImm",
        Instruction::TestGtIntImm(..) => "TestGtIntImm",
        Instruction::TestGeIntImm(..) => "TestGeIntImm",
        Instruction::TestLtFloat(..) => "TestLtFloat",
        Instruction::TestLeFloat(..) => "TestLeFloat",
        Instruction::TestGtFloat(..) => "TestGtFloat",
        Instruction::TestGeFloat(..) => "TestGeFloat",
        Instruction::Return(..) => "Return",
        Instruction::ReturnNull => "ReturnNull",
        Instruction::Call(..) => "Call",
        Instruction::CallDirect(..) => "CallDirect",
        Instruction::CallNative(..) => "CallNative",
        Instruction::CallMethod(..) => "CallMethod",
        Instruction::NullCoalesce(..) => "NullCoalesce",
        Instruction::Concat(..) => "Concat",
        Instruction::MakeArray(..) => "MakeArray",
        Instruction::MakeDict(..) => "MakeDict",
        Instruction::GetIndex(..) => "GetIndex",
        Instruction::SetIndex(..) => "SetIndex",
        Instruction::Spread(..) => "Spread",
        Instruction::GetField(..) => "GetField",
        Instruction::SetField(..) => "SetField",
        Instruction::MakeStruct(..) => "MakeStruct",
        Instruction::MakeClass(..) => "MakeClass",
        Instruction::LoadUpvalue(..) => "LoadUpvalue",
        Instruction::StoreUpvalue(..) => "StoreUpvalue",
        Instruction::MakeClosure(..) => "MakeClosure",
        Instruction::CloseUpvalue(..) => "CloseUpvalue",
        Instruction::StartCoroutine(..) => "StartCoroutine",
        Instruction::Yield => "Yield",
        Instruction::YieldSeconds(..) => "YieldSeconds",
        Instruction::YieldFrames(..) => "YieldFrames",
        Instruction::YieldUntil(..) => "YieldUntil",
        Instruction::YieldCoroutine(..) => "YieldCoroutine",
        Instruction::QAddInt(..) => "QAddInt",
        Instruction::QAddFloat(..) => "QAddFloat",
        Instruction::QSubInt(..) => "QSubInt",
        Instruction::QSubFloat(..) => "QSubFloat",
        Instruction::QMulInt(..) => "QMulInt",
        Instruction::QMulFloat(..) => "QMulFloat",
        Instruction::QDivInt(..) => "QDivInt",
        Instruction::QDivFloat(..) => "QDivFloat",
        Instruction::QLtInt(..) => "QLtInt",
        Instruction::QLtFloat(..) => "QLtFloat",
        Instruction::QLeInt(..) => "QLeInt",
        Instruction::QLeFloat(..) => "QLeFloat",
        Instruction::QGtInt(..) => "QGtInt",
        Instruction::QGtFloat(..) => "QGtFloat",
        Instruction::QGeInt(..) => "QGeInt",
        Instruction::QGeFloat(..) => "QGeFloat",
        Instruction::QEqInt(..) => "QEqInt",
        Instruction::QEqFloat(..) => "QEqFloat",
        Instruction::QNeInt(..) => "QNeInt",
        Instruction::QNeFloat(..) => "QNeFloat",
        Instruction::IntToFloat(..) => "IntToFloat",
        Instruction::TailCallDirect(..) => "TailCallDirect",
        #[cfg(feature = "mobile-aosoa")]
        Instruction::ConvertToAoSoA(..) => "ConvertToAoSoA",
    }
}

fn compile(source: &str) -> (Vec<Vec<Instruction>>, Vec<String>) {
    let tokens = Lexer::new(source).tokenize().unwrap();
    let stmts = Parser::new(tokens).parse_program().unwrap();
    let mut compiler = Compiler::new();
    for stmt in &stmts {
        compiler.compile_stmt(stmt).unwrap();
    }
    let (chunk, functions, _, _) = compiler.into_parts();

    let mut all_chunks: Vec<Vec<Instruction>> = Vec::new();
    let mut chunk_names: Vec<String> = Vec::new();

    all_chunks.push(chunk.instructions().to_vec());
    chunk_names.push("main".to_string());

    for f in &functions {
        all_chunks.push(f.chunk.instructions().to_vec());
        chunk_names.push(f.name.clone());
    }

    (all_chunks, chunk_names)
}

/// Estimate execution weight: how many times each function body runs.
/// For recursive functions, we estimate based on the benchmark.
fn execution_weight(bench_name: &str, func_name: &str) -> u64 {
    match (bench_name, func_name) {
        // fib(28) makes ~1M calls total
        ("fibonacci", "fib") => 1_000_000,
        ("fibonacci", "main") => 1,

        // binary_trees: make called ~200*511=102k times, check ~200*511=102k
        ("binary_trees", "make") => 100_000,
        ("binary_trees", "check") => 100_000,
        ("binary_trees", "main") => 1,

        // permute(9): 9! = 362880 calls to permute
        ("permute", "permute") => 362_880,
        ("permute", "main") => 1,

        // mandelbrot: main loop body runs 100*100*50 = 500k times
        ("mandelbrot", "mandelbrot") => 1,
        ("mandelbrot", "main") => 1,

        // sieve: inner loop runs ~O(n*log(log(n))) times
        ("sieve", "sieve") => 1,
        ("sieve", "main") => 1,

        // queens: solve runs ~O(8^8) in worst case, ~100k typical
        ("queens", "queens") => 1,
        ("queens", "valid") => 100_000,
        ("queens", "solve") => 10_000,
        ("queens", "main") => 1,

        // loop_sum: main body runs 10k times
        ("loop_sum", "main") => 1,

        _ => 1,
    }
}

fn main() {
    let benchmarks = [
        ("fibonacci", FIBONACCI),
        ("binary_trees", BINARY_TREES),
        ("permute", PERMUTE),
        ("mandelbrot", MANDELBROT),
        ("sieve", SIEVE),
        ("queens", QUEENS),
        ("loop_sum", LOOP_SUM),
    ];

    let mut global_pairs: HashMap<String, u64> = HashMap::new();
    let mut global_triples: HashMap<String, u64> = HashMap::new();

    for (bench_name, source) in &benchmarks {
        println!("\n===== {bench_name} =====");
        let (chunks, names) = compile(source);

        for (chunk_idx, instrs) in chunks.iter().enumerate() {
            let func_name = &names[chunk_idx];
            let weight = execution_weight(bench_name, func_name);

            println!(
                "  [{func_name}] ({} instructions, weight={weight})",
                instrs.len()
            );

            // Print the instruction listing for small functions
            if instrs.len() <= 50 {
                for (i, instr) in instrs.iter().enumerate() {
                    println!("    {i:3}: {instr:?}");
                }
            }

            // Count pairs
            for w in instrs.windows(2) {
                let pair = format!("{} + {}", opcode_name(&w[0]), opcode_name(&w[1]));
                *global_pairs.entry(pair).or_default() += weight;
            }

            // Count triples
            for w in instrs.windows(3) {
                let triple = format!(
                    "{} + {} + {}",
                    opcode_name(&w[0]),
                    opcode_name(&w[1]),
                    opcode_name(&w[2])
                );
                *global_triples.entry(triple).or_default() += weight;
            }
        }
    }

    // Sort by weighted frequency
    let mut pairs: Vec<_> = global_pairs.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1));

    let mut triples: Vec<_> = global_triples.into_iter().collect();
    triples.sort_by(|a, b| b.1.cmp(&a.1));

    println!("\n\n===== TOP 30 INSTRUCTION PAIRS (weighted by execution frequency) =====");
    for (i, (pair, count)) in pairs.iter().take(30).enumerate() {
        println!("  {i:2}. {count:>12} | {pair}");
    }

    println!("\n\n===== TOP 30 INSTRUCTION TRIPLES (weighted by execution frequency) =====");
    for (i, (triple, count)) in triples.iter().take(30).enumerate() {
        println!("  {i:2}. {count:>12} | {triple}");
    }

    // Also show unweighted static counts (what the bytecode looks like)
    println!("\n\n===== TOP 20 INSTRUCTION PAIRS (static / unweighted) =====");
    let (chunks_all, _) = {
        let mut all_pairs: HashMap<String, u64> = HashMap::new();
        for (_, source) in &benchmarks {
            let (chunks, _) = compile(source);
            for instrs in &chunks {
                for w in instrs.windows(2) {
                    let pair = format!("{} + {}", opcode_name(&w[0]), opcode_name(&w[1]));
                    *all_pairs.entry(pair).or_default() += 1;
                }
            }
        }
        let mut sorted: Vec<_> = all_pairs.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        for (i, (pair, count)) in sorted.iter().take(20).enumerate() {
            println!("  {i:2}. {count:>6} | {pair}");
        }
        (Vec::<Vec<Instruction>>::new(), Vec::<String>::new())
    };
    let _ = chunks_all;
}
