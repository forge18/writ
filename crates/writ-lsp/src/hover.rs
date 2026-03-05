use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};

use crate::convert::position_to_offset;
use crate::document::WorldState;

/// Handles `textDocument/hover` requests.
pub fn handle_hover(world: &WorldState, params: HoverParams) -> Option<Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;
    let doc = world.get_document(uri)?;

    let offset = position_to_offset(&doc.source, pos)?;
    let name = extract_identifier_at_offset(&doc.source, offset);
    if name.is_empty() {
        return None;
    }

    let checker = doc.type_checker.as_ref()?;
    let env = checker.env();

    // Look up the identifier in the type environment.
    let type_str = if let Some(info) = env.lookup(&name) {
        info.ty.to_string()
    } else {
        // Check type registry for class/trait/enum names.
        let registry = checker.registry();
        if registry.get_class(&name).is_some() {
            format!("class {name}")
        } else if registry.get_trait(&name).is_some() {
            format!("trait {name}")
        } else if registry.get_enum(&name).is_some() {
            format!("enum {name}")
        } else if registry.get_struct(&name).is_some() {
            format!("struct {name}")
        } else {
            return None;
        }
    };

    // Extract doc comment from source (lines starting with // above the declaration).
    let doc_comment = extract_doc_comment(&doc.source, &name);

    let mut value = format!("```writ\n{name}: {type_str}\n```");
    if let Some(comment) = doc_comment {
        value.push_str(&format!("\n\n{comment}"));
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    })
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

/// Scans the source for `//` comment lines immediately above the declaration of `name`.
fn extract_doc_comment(source: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();

    // Find the line containing the declaration of name (let/var/const/func/class/trait/enum).
    let decl_line = lines.iter().position(|line| {
        let trimmed = line.trim();
        // Match common declaration patterns.
        trimmed.starts_with(&format!("let {name}"))
            || trimmed.starts_with(&format!("var {name}"))
            || trimmed.starts_with(&format!("const {name}"))
            || trimmed.starts_with(&format!("func {name}"))
            || trimmed.starts_with(&format!("class {name}"))
            || trimmed.starts_with(&format!("trait {name}"))
            || trimmed.starts_with(&format!("enum {name}"))
            || trimmed.starts_with(&format!("struct {name}"))
            || trimmed.starts_with(&format!("public {name}"))
            || trimmed.starts_with(&format!("private {name}"))
            || trimmed.starts_with(&format!("export class {name}"))
            || trimmed.starts_with(&format!("export func {name}"))
    })?;

    // Collect consecutive // comment lines above the declaration.
    let mut comment_lines = Vec::new();
    let mut i = decl_line;
    while i > 0 {
        i -= 1;
        let trimmed = lines[i].trim();
        if let Some(comment) = trimmed.strip_prefix("//") {
            comment_lines.push(comment.trim());
        } else {
            break;
        }
    }

    if comment_lines.is_empty() {
        return None;
    }

    comment_lines.reverse();
    Some(comment_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_doc_comment_above_func() {
        let source =
            "// Adds two numbers.\n// Returns the sum.\nfunc add(a: int, b: int) -> int {}";
        let comment = extract_doc_comment(source, "add");
        assert_eq!(
            comment,
            Some("Adds two numbers.\nReturns the sum.".to_string())
        );
    }

    #[test]
    fn extract_doc_comment_none_when_missing() {
        let source = "func add(a: int, b: int) -> int {}";
        let comment = extract_doc_comment(source, "add");
        assert_eq!(comment, None);
    }
}
