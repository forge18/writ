use lsp_types::{
    Position, RenameParams, TextDocumentIdentifier, TextDocumentPositionParams, Uri,
    WorkDoneProgressParams,
};
use writ_lsp::WorldState;

fn analyze_source(source: &str) -> (WorldState, Uri) {
    let uri: Uri = "file:///test.writ".parse().unwrap();
    let mut world = WorldState::new();
    world.open_document(uri.clone(), source.to_string());
    (world, uri)
}

fn make_rename_params(uri: &Uri, line: u32, character: u32, new_name: &str) -> RenameParams {
    RenameParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line, character },
        },
        new_name: new_name.to_string(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
}

#[test]
fn test_rename_updates_all_references() {
    let source = "let x = 42\nlet y = x + 1";
    let (world, uri) = analyze_source(source);

    // Rename 'x' (line 0, character 4) to 'value'
    let params = make_rename_params(&uri, 0, 4, "value");
    let result = writ_lsp::rename::handle_rename(&world, params);

    assert!(result.is_some(), "expected rename result");
    let edit = result.unwrap();
    let changes = edit.changes.expect("expected changes in workspace edit");
    let edits = changes.get(&uri).expect("expected edits for the file");

    // Should have at least 2 edits: the declaration and the usage
    assert!(
        edits.len() >= 2,
        "expected at least 2 rename edits (declaration + usage), got: {}",
        edits.len()
    );

    // All edits should have the new name
    for edit in edits {
        assert_eq!(edit.new_text, "value");
    }
}
