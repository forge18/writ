use std::env;
use std::process;

use writ::Writ;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: writ <file.writ>");
        process::exit(1);
    }

    let path = &args[1];
    let mut writ = Writ::new();

    if let Err(e) = writ.load(path) {
        eprintln!("{e}");
        process::exit(1);
    }
}
