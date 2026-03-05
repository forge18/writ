fn main() {
    if let Err(e) = writ_lsp::run_server() {
        eprintln!("writ-lsp error: {e}");
        std::process::exit(1);
    }
}
