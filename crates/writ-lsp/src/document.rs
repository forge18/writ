use std::collections::HashMap;

use lsp_types::{Diagnostic, Uri};
use writ_parser::Stmt;
use writ_types::TypeChecker;

use crate::analysis;

/// Per-file cached state maintained by the LSP server.
pub struct DocumentState {
    /// Raw source text (latest version from the editor).
    pub source: String,
    /// AST from the most recent successful parse.
    pub stmts: Option<Vec<Stmt>>,
    /// Type checker state after analysis (for completions, hover, etc.).
    pub type_checker: Option<TypeChecker>,
    /// Collected diagnostics from all pipeline stages.
    pub diagnostics: Vec<Diagnostic>,
}

/// Central state for all open documents.
#[derive(Default)]
pub struct WorldState {
    documents: HashMap<Uri, DocumentState>,
}

impl WorldState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a document and runs analysis.
    pub fn open_document(&mut self, uri: Uri, source: String) {
        let file_path = uri_to_file_path(&uri);
        let result = analysis::analyze(&source, &file_path);
        self.documents.insert(
            uri,
            DocumentState {
                source,
                stmts: result.stmts,
                type_checker: result.type_checker,
                diagnostics: result.diagnostics,
            },
        );
    }

    /// Updates a document with new source text and re-runs analysis.
    pub fn update_document(&mut self, uri: Uri, source: String) {
        self.open_document(uri, source);
    }

    /// Closes a document and removes it from state.
    pub fn close_document(&mut self, uri: &Uri) {
        self.documents.remove(uri);
    }

    /// Returns the state for a document, if it is open.
    pub fn get_document(&self, uri: &Uri) -> Option<&DocumentState> {
        self.documents.get(uri)
    }
}

/// Extracts a file path string from a URI for span tracking.
fn uri_to_file_path(uri: &Uri) -> String {
    let s = uri.as_str();
    s.strip_prefix("file://").unwrap_or(s).to_string()
}
