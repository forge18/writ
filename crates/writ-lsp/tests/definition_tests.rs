use lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, PartialResultParams, Position,
    TextDocumentIdentifier, TextDocumentPositionParams, Uri, WorkDoneProgressParams,
};
use writ_lsp::WorldState;

fn analyze_source(source: &str) -> (WorldState, Uri) {
    let uri: Uri = "file:///test.writ".parse().unwrap();
    let mut world = WorldState::new();
    world.open_document(uri.clone(), source.to_string());
    (world, uri)
}

fn make_definition_params(uri: &Uri, line: u32, character: u32) -> GotoDefinitionParams {
    GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line, character },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

#[test]
fn test_go_to_definition_local_var() {
    let source = "let x = 42\nlet y = x";
    let (world, uri) = analyze_source(source);

    // Position on 'x' in "let y = x" — line 1, character 8
    let params = make_definition_params(&uri, 1, 8);
    let result = writ_lsp::definition::handle_goto_definition(&world, params);

    assert!(result.is_some(), "expected to find definition of 'x'");
    match result.unwrap() {
        GotoDefinitionResponse::Scalar(location) => {
            // Should point to line 0 where "let x = 42" is
            assert_eq!(
                location.range.start.line, 0,
                "definition should be on line 0"
            );
        }
        other => panic!("expected Scalar response, got: {other:?}"),
    }
}

#[test]
fn test_go_to_definition_imported_name() {
    // Test that go-to-definition works for function names.
    let source = "func add(a: int, b: int) -> int {\n    return a + b\n}\nlet result = add(1, 2)";
    let (world, uri) = analyze_source(source);

    // Position on 'add' in "let result = add(1, 2)" — line 3, character 13
    let params = make_definition_params(&uri, 3, 13);
    let result = writ_lsp::definition::handle_goto_definition(&world, params);

    assert!(result.is_some(), "expected to find definition of 'add'");
    match result.unwrap() {
        GotoDefinitionResponse::Scalar(location) => {
            // Should point to line 0 where "func add(...)" is
            assert_eq!(
                location.range.start.line, 0,
                "definition should be on line 0"
            );
        }
        other => panic!("expected Scalar response, got: {other:?}"),
    }
}
