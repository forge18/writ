use std::collections::HashMap;

use lsp_types::{RenameParams, TextEdit, WorkspaceEdit};

use crate::convert::{position_to_offset, span_to_range};
use crate::document::WorldState;

/// Handles `textDocument/rename` requests.
// Uri contains interior mutability for caching, but Hash/Eq are based on the
// immutable string content. This is safe and matches how the LSP protocol works.
#[allow(clippy::mutable_key_type)]
pub fn handle_rename(world: &WorldState, params: RenameParams) -> Option<WorkspaceEdit> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;
    let doc = world.get_document(uri)?;

    let offset = position_to_offset(&doc.source, pos)?;
    let name = extract_identifier_at_offset(&doc.source, offset);
    if name.is_empty() {
        return None;
    }

    let stmts = doc.stmts.as_ref()?;
    let new_name = &params.new_name;

    // Collect all locations of this identifier in the AST.
    let mut edits = Vec::new();
    collect_rename_edits(stmts, &name, new_name, &mut edits);

    if edits.is_empty() {
        return None;
    }

    let mut changes = HashMap::new();
    changes.insert(uri.clone(), edits);

    Some(WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

/// Walks the AST and collects text edits for renaming `old_name` to `new_name`.
fn collect_rename_edits(
    stmts: &[writ::parser::Stmt],
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<TextEdit>,
) {
    for stmt in stmts {
        collect_from_stmt(stmt, old_name, new_name, edits);
    }
}

fn collect_from_stmt(
    stmt: &writ::parser::Stmt,
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<TextEdit>,
) {
    use writ::parser::StmtKind;

    match &stmt.kind {
        StmtKind::Let {
            name, initializer, ..
        }
        | StmtKind::Var {
            name, initializer, ..
        } => {
            if name == old_name {
                edits.push(TextEdit {
                    range: span_to_range(&stmt.span),
                    new_text: new_name.to_string(),
                });
            }
            collect_from_expr(initializer, old_name, new_name, edits);
        }
        StmtKind::Const { name, initializer } => {
            if name == old_name {
                edits.push(TextEdit {
                    range: span_to_range(&stmt.span),
                    new_text: new_name.to_string(),
                });
            }
            collect_from_expr(initializer, old_name, new_name, edits);
        }
        StmtKind::ExprStmt(expr) => {
            collect_from_expr(expr, old_name, new_name, edits);
        }
        StmtKind::Return(Some(expr)) => {
            collect_from_expr(expr, old_name, new_name, edits);
        }
        StmtKind::Assignment { target, value, .. } => {
            collect_from_expr(target, old_name, new_name, edits);
            collect_from_expr(value, old_name, new_name, edits);
        }
        StmtKind::Func(func) => {
            if func.name == old_name {
                edits.push(TextEdit {
                    range: span_to_range(&stmt.span),
                    new_text: new_name.to_string(),
                });
            }
            collect_rename_edits(&func.body, old_name, new_name, edits);
        }
        StmtKind::Class(class) => {
            if class.name == old_name {
                edits.push(TextEdit {
                    range: span_to_range(&stmt.span),
                    new_text: new_name.to_string(),
                });
            }
            for method in &class.methods {
                collect_rename_edits(&method.body, old_name, new_name, edits);
            }
        }
        StmtKind::Block(stmts) => {
            collect_rename_edits(stmts, old_name, new_name, edits);
        }
        StmtKind::If {
            condition,
            then_block,
            else_branch,
        } => {
            collect_from_expr(condition, old_name, new_name, edits);
            collect_rename_edits(then_block, old_name, new_name, edits);
            if let Some(branch) = else_branch {
                match branch {
                    writ::parser::ElseBranch::ElseIf(s) => {
                        collect_from_stmt(s, old_name, new_name, edits);
                    }
                    writ::parser::ElseBranch::ElseBlock(stmts) => {
                        collect_rename_edits(stmts, old_name, new_name, edits);
                    }
                }
            }
        }
        StmtKind::While { condition, body } => {
            collect_from_expr(condition, old_name, new_name, edits);
            collect_rename_edits(body, old_name, new_name, edits);
        }
        StmtKind::For {
            variable,
            iterable,
            body,
        } => {
            if variable == old_name {
                edits.push(TextEdit {
                    range: span_to_range(&stmt.span),
                    new_text: new_name.to_string(),
                });
            }
            collect_from_expr(iterable, old_name, new_name, edits);
            collect_rename_edits(body, old_name, new_name, edits);
        }
        StmtKind::Export(inner) => {
            collect_from_stmt(inner, old_name, new_name, edits);
        }
        StmtKind::When { subject, arms } => {
            if let Some(expr) = subject {
                collect_from_expr(expr, old_name, new_name, edits);
            }
            for arm in arms {
                match &arm.body {
                    writ::parser::WhenBody::Expr(expr) => {
                        collect_from_expr(expr, old_name, new_name, edits);
                    }
                    writ::parser::WhenBody::Block(stmts) => {
                        collect_rename_edits(stmts, old_name, new_name, edits);
                    }
                }
            }
        }
        StmtKind::Start(expr) => {
            collect_from_expr(expr, old_name, new_name, edits);
        }
        _ => {}
    }
}

fn collect_from_expr(
    expr: &writ::parser::Expr,
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<TextEdit>,
) {
    use writ::parser::ExprKind;

    match &expr.kind {
        ExprKind::Identifier(ident) => {
            if ident == old_name {
                edits.push(TextEdit {
                    range: span_to_range(&expr.span),
                    new_text: new_name.to_string(),
                });
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_from_expr(lhs, old_name, new_name, edits);
            collect_from_expr(rhs, old_name, new_name, edits);
        }
        ExprKind::Unary { operand, .. } => {
            collect_from_expr(operand, old_name, new_name, edits);
        }
        ExprKind::Grouped(inner) => {
            collect_from_expr(inner, old_name, new_name, edits);
        }
        ExprKind::Call { callee, args } => {
            collect_from_expr(callee, old_name, new_name, edits);
            for arg in args {
                match arg {
                    writ::parser::CallArg::Positional(expr) => {
                        collect_from_expr(expr, old_name, new_name, edits);
                    }
                    writ::parser::CallArg::Named { value, .. } => {
                        collect_from_expr(value, old_name, new_name, edits);
                    }
                }
            }
        }
        ExprKind::MemberAccess { object, .. } | ExprKind::SafeAccess { object, .. } => {
            collect_from_expr(object, old_name, new_name, edits);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_from_expr(condition, old_name, new_name, edits);
            collect_from_expr(then_expr, old_name, new_name, edits);
            collect_from_expr(else_expr, old_name, new_name, edits);
        }
        ExprKind::NullCoalesce { lhs, rhs }
        | ExprKind::Range {
            start: lhs,
            end: rhs,
            ..
        } => {
            collect_from_expr(lhs, old_name, new_name, edits);
            collect_from_expr(rhs, old_name, new_name, edits);
        }
        ExprKind::Lambda { body, .. } => match body {
            writ::parser::LambdaBody::Expr(expr) => {
                collect_from_expr(expr, old_name, new_name, edits);
            }
            writ::parser::LambdaBody::Block(stmts) => {
                collect_rename_edits(stmts, old_name, new_name, edits);
            }
        },
        ExprKind::ArrayLiteral(elements) => {
            for element in elements {
                match element {
                    writ::parser::ArrayElement::Expr(expr)
                    | writ::parser::ArrayElement::Spread(expr) => {
                        collect_from_expr(expr, old_name, new_name, edits);
                    }
                }
            }
        }
        ExprKind::DictLiteral(elements) => {
            for element in elements {
                match element {
                    writ::parser::DictElement::KeyValue { key, value } => {
                        collect_from_expr(key, old_name, new_name, edits);
                        collect_from_expr(value, old_name, new_name, edits);
                    }
                    writ::parser::DictElement::Spread(expr) => {
                        collect_from_expr(expr, old_name, new_name, edits);
                    }
                }
            }
        }
        ExprKind::ErrorPropagate(inner) | ExprKind::Cast { expr: inner, .. } => {
            collect_from_expr(inner, old_name, new_name, edits);
        }
        ExprKind::StringInterpolation(segments) => {
            for segment in segments {
                if let writ::parser::InterpolationSegment::Expression(expr) = segment {
                    collect_from_expr(expr, old_name, new_name, edits);
                }
            }
        }
        ExprKind::Tuple(exprs) => {
            for expr in exprs {
                collect_from_expr(expr, old_name, new_name, edits);
            }
        }
        ExprKind::Yield(Some(inner)) => {
            collect_from_expr(inner, old_name, new_name, edits);
        }
        ExprKind::When { subject, arms } => {
            if let Some(subj) = subject {
                collect_from_expr(subj, old_name, new_name, edits);
            }
            for arm in arms {
                if let writ::parser::WhenBody::Block(stmts) = &arm.body {
                    for stmt in stmts {
                        collect_from_stmt(stmt, old_name, new_name, edits);
                    }
                } else if let writ::parser::WhenBody::Expr(expr) = &arm.body {
                    collect_from_expr(expr, old_name, new_name, edits);
                }
            }
        }
        ExprKind::Index { object, index } => {
            collect_from_expr(object, old_name, new_name, edits);
            collect_from_expr(index, old_name, new_name, edits);
        }
        ExprKind::NamespaceAccess { .. }
        | ExprKind::Literal(_)
        | ExprKind::Yield(None)
        | ExprKind::Super { .. } => {}
    }
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
