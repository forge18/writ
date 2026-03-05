use lsp_types::{
    HoverParams, Position, TextDocumentIdentifier, TextDocumentPositionParams, Uri,
    WorkDoneProgressParams,
};
use writ_lsp::WorldState;

fn analyze_source(source: &str) -> (WorldState, Uri) {
    let uri: Uri = "file:///test.writ".parse().unwrap();
    let mut world = WorldState::new();
    world.open_document(uri.clone(), source.to_string());
    (world, uri)
}

fn make_hover_params(uri: &Uri, line: u32, character: u32) -> HoverParams {
    HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line, character },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
}

#[test]
fn test_hover_shows_type() {
    let source = "let x = 42\nx";
    let (world, uri) = analyze_source(source);

    // Hover over 'x' on line 1, character 0
    let params = make_hover_params(&uri, 1, 0);
    let result = writ_lsp::hover::handle_hover(&world, params);

    assert!(result.is_some(), "expected hover result for 'x'");
    let hover = result.unwrap();
    match hover.contents {
        lsp_types::HoverContents::Markup(markup) => {
            assert!(
                markup.value.contains("int"),
                "hover should show 'int' type, got: {}",
                markup.value
            );
        }
        other => panic!("expected Markup content, got: {other:?}"),
    }
}

#[test]
fn test_hover_shows_doc_comment() {
    let source =
        "// Adds two numbers.\nfunc add(a: int, b: int) -> int {\n    return a + b\n}\nadd";
    let (world, uri) = analyze_source(source);

    // Hover over 'add' on line 4, character 0
    let params = make_hover_params(&uri, 4, 0);
    let result = writ_lsp::hover::handle_hover(&world, params);

    assert!(result.is_some(), "expected hover result for 'add'");
    let hover = result.unwrap();
    match hover.contents {
        lsp_types::HoverContents::Markup(markup) => {
            assert!(
                markup.value.contains("Adds two numbers"),
                "hover should include doc comment, got: {}",
                markup.value
            );
        }
        other => panic!("expected Markup content, got: {other:?}"),
    }
}
