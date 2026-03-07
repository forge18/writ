use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location};

use crate::convert::{position_to_offset, span_to_range};
use crate::document::WorldState;

/// Handles `textDocument/definition` requests.
pub fn handle_goto_definition(
    world: &WorldState,
    params: GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let uri = &params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;
    let doc = world.get_document(uri)?;

    let offset = position_to_offset(&doc.source, pos)?;
    let name = extract_identifier_at_offset(&doc.source, offset);
    if name.is_empty() {
        return None;
    }

    let stmts = doc.stmts.as_ref()?;

    // Search the AST for the declaration of this name.
    for stmt in stmts {
        if let Some(span) = find_declaration_span(stmt, &name) {
            return Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range: span_to_range(&span),
            }));
        }
    }

    None
}

/// Extracts the identifier at the given byte offset.
fn extract_identifier_at_offset(source: &str, offset: usize) -> String {
    let bytes = source.as_bytes();
    if offset >= bytes.len() {
        return String::new();
    }

    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    if start == end {
        return String::new();
    }

    source[start..end].to_string()
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Searches a statement (and its children) for a declaration matching `name`.
/// Returns the span of the declaration if found.
fn find_declaration_span(stmt: &writ::parser::Stmt, name: &str) -> Option<writ::lexer::Span> {
    use writ::parser::StmtKind;

    match &stmt.kind {
        StmtKind::Let {
            name: decl_name, ..
        }
        | StmtKind::Var {
            name: decl_name, ..
        }
        | StmtKind::Const {
            name: decl_name, ..
        } => {
            if decl_name == name {
                return Some(stmt.span.clone());
            }
        }
        StmtKind::Func(func) => {
            if func.name == name {
                return Some(stmt.span.clone());
            }
            // Search inside function body.
            for body_stmt in &func.body {
                if let Some(span) = find_declaration_span(body_stmt, name) {
                    return Some(span);
                }
            }
        }
        StmtKind::Class(class) => {
            if class.name == name {
                return Some(stmt.span.clone());
            }
        }
        StmtKind::Trait(trait_decl) => {
            if trait_decl.name == name {
                return Some(stmt.span.clone());
            }
        }
        StmtKind::Enum(enum_decl) => {
            if enum_decl.name == name {
                return Some(stmt.span.clone());
            }
        }
        StmtKind::Block(stmts) => {
            for s in stmts {
                if let Some(span) = find_declaration_span(s, name) {
                    return Some(span);
                }
            }
        }
        StmtKind::If {
            then_block,
            else_branch,
            ..
        } => {
            for s in then_block {
                if let Some(span) = find_declaration_span(s, name) {
                    return Some(span);
                }
            }
            if let Some(branch) = else_branch {
                match branch {
                    writ::parser::ElseBranch::ElseIf(s) => {
                        if let Some(span) = find_declaration_span(s, name) {
                            return Some(span);
                        }
                    }
                    writ::parser::ElseBranch::ElseBlock(stmts) => {
                        for s in stmts {
                            if let Some(span) = find_declaration_span(s, name) {
                                return Some(span);
                            }
                        }
                    }
                }
            }
        }
        StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
            for s in body {
                if let Some(span) = find_declaration_span(s, name) {
                    return Some(span);
                }
            }
        }
        StmtKind::Export(inner) => {
            if let Some(span) = find_declaration_span(inner, name) {
                return Some(span);
            }
        }
        _ => {}
    }
    None
}
