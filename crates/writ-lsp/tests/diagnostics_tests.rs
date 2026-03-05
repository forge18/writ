use lsp_types::Uri;
use writ_lsp::WorldState;

fn analyze_source(source: &str) -> (WorldState, Uri) {
    let uri: Uri = "file:///test.writ".parse().unwrap();
    let mut world = WorldState::new();
    world.open_document(uri.clone(), source.to_string());
    (world, uri)
}

#[test]
fn test_diagnostics_on_type_error() {
    let (world, uri) = analyze_source("let x: int = 3.14");
    let doc = world.get_document(&uri).unwrap();
    assert!(
        !doc.diagnostics.is_empty(),
        "expected at least one diagnostic for type mismatch"
    );
    let msg = &doc.diagnostics[0].message;
    assert!(
        msg.contains("mismatch") || msg.contains("expected"),
        "diagnostic should mention type mismatch, got: {msg}"
    );
}
