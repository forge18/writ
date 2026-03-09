use lsp_types::{CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse};

use crate::analysis;
use crate::convert::position_to_offset;
use crate::document::WorldState;

/// Writ language keywords for completion suggestions.
const KEYWORDS: &[&str] = &[
    "class", "trait", "enum", "struct", "func", "let", "var", "const", "public", "private",
    "static", "extends", "with", "import", "export", "return", "if", "else", "when", "while",
    "for", "in", "break", "continue", "is", "as", "self", "start", "yield", "true", "false",
    "null",
];

/// Handles `textDocument/completion` requests.
pub fn handle_completion(
    world: &WorldState,
    params: CompletionParams,
) -> Option<CompletionResponse> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;
    let doc = world.get_document(uri)?;

    let offset = position_to_offset(&doc.source, pos)?;

    // Determine trigger context by looking at the character before the cursor.
    let before_cursor = if offset > 0 {
        doc.source.as_bytes().get(offset - 1).copied()
    } else {
        None
    };

    let mut items = Vec::new();

    match before_cursor {
        Some(b'.') => {
            // Dot trigger: find the expression before the dot and list fields/methods.
            if let (Some(stmts), Some(checker)) = (&doc.stmts, &doc.type_checker) {
                complete_dot_access(&doc.source, offset, stmts, checker, &mut items);
            }
            // If parse failed (stmts is None), try re-analyzing with the incomplete
            // line removed so type info is available for the identifier before the dot.
            if items.is_empty()
                && doc.stmts.is_none()
                && let Some(fixed_checker) = analyze_without_line(&doc.source, offset)
            {
                complete_dot_access(&doc.source, offset, &[], &fixed_checker, &mut items);
            }
        }
        Some(b':') => {
            // `::` trigger: complete namespace members.
            if offset >= 2 && doc.source.as_bytes().get(offset - 2) == Some(&b':') {
                if let Some(checker) = &doc.type_checker {
                    complete_namespace_access(&doc.source, offset, checker, &mut items);
                }
            }
        }
        _ => {
            // Identifier prefix or general completion.
            let prefix = extract_identifier_prefix(&doc.source, offset);
            complete_general(doc, &prefix, &mut items);
        }
    }

    if items.is_empty() {
        None
    } else {
        Some(CompletionResponse::Array(items))
    }
}

/// Completes after a `.` — resolves the type of the expression before the dot
/// and lists its fields and methods.
fn complete_dot_access(
    source: &str,
    offset: usize,
    _stmts: &[writ::parser::Stmt],
    checker: &writ::types::TypeChecker,
    items: &mut Vec<CompletionItem>,
) {
    // Walk backwards to find the identifier before the dot.
    let before_dot = if offset >= 2 { offset - 2 } else { return };
    let name = extract_identifier_at(source, before_dot);
    if name.is_empty() {
        return;
    }

    // Look up the identifier's type in the type environment.
    let env = checker.env();
    let ty = match env.lookup(&name) {
        Some(info) => &info.ty,
        None => return,
    };

    let registry = checker.registry();

    match ty {
        writ::types::Type::Class(type_name) => {
            // Add fields (with inheritance).
            for field in registry.all_fields(type_name) {
                items.push(CompletionItem {
                    label: field.name.clone(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(field.ty.to_string()),
                    ..Default::default()
                });
            }
            // Add methods (with inheritance).
            for method in registry.all_methods(type_name) {
                if method.is_static {
                    continue;
                }
                items.push(CompletionItem {
                    label: method.name.clone(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(format!(
                        "({}) -> {}",
                        format_params(&method.params),
                        method.return_type
                    )),
                    ..Default::default()
                });
            }
        }
        writ::types::Type::Struct(type_name) => {
            if let Some(info) = registry.get_struct(type_name) {
                for field in &info.fields {
                    items.push(CompletionItem {
                        label: field.name.clone(),
                        kind: Some(CompletionItemKind::FIELD),
                        detail: Some(field.ty.to_string()),
                        ..Default::default()
                    });
                }
                for method in &info.methods {
                    if method.is_static {
                        continue;
                    }
                    items.push(CompletionItem {
                        label: method.name.clone(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(format!(
                            "({}) -> {}",
                            format_params(&method.params),
                            method.return_type
                        )),
                        ..Default::default()
                    });
                }
            }
        }
        _ => {}
    }
}

/// Completes after `::` — resolves the namespace alias before `::` and lists module exports.
fn complete_namespace_access(
    source: &str,
    offset: usize,
    checker: &writ::types::TypeChecker,
    items: &mut Vec<CompletionItem>,
) {
    // Walk back past `::` (2 bytes) to find the namespace identifier.
    let ns_end = offset - 2;
    let ns = extract_identifier_at(source, if ns_end > 0 { ns_end - 1 } else { 0 });
    if ns.is_empty() {
        return;
    }

    let Some(module_path) = checker.namespace_aliases().get(&ns) else {
        return;
    };
    let Some(exports) = checker.module_registry().get_module(module_path) else {
        return;
    };

    for (name, ty) in exports {
        let kind = match ty {
            writ::types::Type::Class(_) => CompletionItemKind::CLASS,
            writ::types::Type::Function { .. } => CompletionItemKind::FUNCTION,
            writ::types::Type::Enum(_) => CompletionItemKind::ENUM,
            writ::types::Type::Struct(_) => CompletionItemKind::STRUCT,
            _ => CompletionItemKind::VALUE,
        };
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(kind),
            detail: Some(ty.to_string()),
            ..Default::default()
        });
    }
}

/// Provides general completions: locals, globals, type names, keywords.
fn complete_general(
    doc: &crate::document::DocumentState,
    prefix: &str,
    items: &mut Vec<CompletionItem>,
) {
    // Keywords.
    for &kw in KEYWORDS {
        if prefix.is_empty() || kw.starts_with(prefix) {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
    }

    // Locals and globals from type environment.
    if let Some(checker) = &doc.type_checker {
        let env = checker.env();
        for (name, info) in env.all_visible() {
            if prefix.is_empty() || name.starts_with(prefix) {
                let kind = match &info.ty {
                    writ::types::Type::Function { .. } => CompletionItemKind::FUNCTION,
                    _ => CompletionItemKind::VARIABLE,
                };
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(kind),
                    detail: Some(info.ty.to_string()),
                    ..Default::default()
                });
            }
        }

        // Type names (classes, traits, enums).
        let registry = checker.registry();
        for name in registry.class_names() {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::CLASS),
                    ..Default::default()
                });
            }
        }
        for name in registry.trait_names() {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::INTERFACE),
                    ..Default::default()
                });
            }
        }
        for name in registry.enum_names() {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::ENUM),
                    ..Default::default()
                });
            }
        }
        for name in registry.struct_names() {
            if prefix.is_empty() || name.starts_with(prefix) {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::STRUCT),
                    ..Default::default()
                });
            }
        }
    }
}

/// Extracts the identifier being typed at the cursor position.
fn extract_identifier_prefix(source: &str, offset: usize) -> String {
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    source[start..offset].to_string()
}

/// Extracts the identifier at the given byte offset (searching backwards and forwards).
fn extract_identifier_at(source: &str, offset: usize) -> String {
    let bytes = source.as_bytes();
    if offset >= bytes.len() || !is_ident_char(bytes[offset]) {
        // Try searching backwards from offset.
        let end = offset + 1;
        let mut start = end;
        while start > 0 && is_ident_char(bytes[start - 1]) {
            start -= 1;
        }
        if start < end && end <= bytes.len() {
            return source[start..end].to_string();
        }
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
    source[start..end].to_string()
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn format_params(params: &[writ::types::Type]) -> String {
    params
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Re-analyzes the source with the line containing `offset` removed.
///
/// This allows dot-completion to work even when the current line has a parse error
/// (e.g., `p.` with no member). By removing the incomplete line, the rest of the
/// program parses and type-checks successfully, giving us the type information
/// needed for completions.
fn analyze_without_line(source: &str, offset: usize) -> Option<writ::types::TypeChecker> {
    // Find the line containing the offset and remove it.
    let mut fixed = String::new();
    let mut current_offset = 0;
    for line in source.split('\n') {
        let line_end = current_offset + line.len();
        if !(current_offset..=line_end).contains(&offset) {
            fixed.push_str(line);
            fixed.push('\n');
        }
        current_offset = line_end + 1;
    }

    let result = analysis::analyze(&fixed, "");
    result.type_checker
}
