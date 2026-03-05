use lsp_types::{
    CompletionParams, CompletionResponse, PartialResultParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Uri, WorkDoneProgressParams,
};
use writ_lsp::WorldState;

fn analyze_source(source: &str) -> (WorldState, Uri) {
    let uri: Uri = "file:///test.writ".parse().unwrap();
    let mut world = WorldState::new();
    world.open_document(uri.clone(), source.to_string());
    (world, uri)
}

fn make_completion_params(uri: &Uri, line: u32, character: u32) -> CompletionParams {
    CompletionParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            position: Position { line, character },
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    }
}

fn completion_labels(response: CompletionResponse) -> Vec<String> {
    match response {
        CompletionResponse::Array(items) => items.into_iter().map(|i| i.label).collect(),
        CompletionResponse::List(list) => list.items.into_iter().map(|i| i.label).collect(),
    }
}

#[test]
fn test_completion_class_members() {
    // Source with a class and a variable of that type.
    // After "p." the LSP should suggest fields and methods.
    let source = concat!(
        "class Player {\n",
        "    public health: float = 100.0\n",
        "    public func attack() -> void {}\n",
        "}\n",
        "let p = Player()\n",
        "p.\n",
    );
    let (world, uri) = analyze_source(source);

    // Position at end of "p." on line 5 (0-indexed), character 2
    let params = make_completion_params(&uri, 5, 2);
    let result = writ_lsp::completion::handle_completion(&world, params);

    assert!(result.is_some(), "expected completion results after 'p.'");
    let labels = completion_labels(result.unwrap());
    assert!(
        labels.contains(&"health".to_string()),
        "should suggest 'health' field, got: {labels:?}"
    );
    assert!(
        labels.contains(&"attack".to_string()),
        "should suggest 'attack' method, got: {labels:?}"
    );
}

#[test]
fn test_completion_global_functions() {
    // Register a function (like print) via type checker, then verify it appears.
    // Since we don't register stdlib in the LSP analysis, we test with user-defined functions.
    let source = "func greet() -> void {}\ngr";
    let (world, uri) = analyze_source(source);

    // Position at end of "gr" on line 1, character 2
    let params = make_completion_params(&uri, 1, 2);
    let result = writ_lsp::completion::handle_completion(&world, params);

    assert!(
        result.is_some(),
        "expected completion results for 'gr' prefix"
    );
    let labels = completion_labels(result.unwrap());
    assert!(
        labels.contains(&"greet".to_string()),
        "should suggest 'greet' function, got: {labels:?}"
    );
}

#[test]
fn test_partial_parse_still_provides_completions() {
    // Source with a syntax error — completions should still work (keywords, etc.)
    let source = "let x = \n";
    let (world, uri) = analyze_source(source);

    // Position at start of second line
    let params = make_completion_params(&uri, 1, 0);
    let result = writ_lsp::completion::handle_completion(&world, params);

    assert!(
        result.is_some(),
        "expected completions even with parse error"
    );
    let labels = completion_labels(result.unwrap());
    assert!(
        labels.contains(&"let".to_string()),
        "should at least suggest keywords, got: {labels:?}"
    );
}
