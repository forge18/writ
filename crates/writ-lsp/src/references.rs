use lsp_types::{Location, ReferenceParams};

use crate::convert::{position_to_offset, span_to_range};
use crate::document::WorldState;

/// Handles `textDocument/references` requests.
pub fn handle_references(world: &WorldState, params: ReferenceParams) -> Option<Vec<Location>> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;
    let doc = world.get_document(uri)?;

    let offset = position_to_offset(&doc.source, pos)?;
    let name = extract_identifier_at_offset(&doc.source, offset);
    if name.is_empty() {
        return None;
    }

    let stmts = doc.stmts.as_ref()?;

    let mut locations = Vec::new();
    collect_identifier_locations(stmts, &name, uri, &mut locations);

    if locations.is_empty() {
        None
    } else {
        Some(locations)
    }
}

/// Walks the AST and collects all locations where `name` appears as an identifier.
fn collect_identifier_locations(
    stmts: &[writ_parser::Stmt],
    name: &str,
    uri: &lsp_types::Uri,
    locations: &mut Vec<Location>,
) {
    for stmt in stmts {
        collect_from_stmt(stmt, name, uri, locations);
    }
}

fn collect_from_stmt(
    stmt: &writ_parser::Stmt,
    name: &str,
    uri: &lsp_types::Uri,
    locations: &mut Vec<Location>,
) {
    use writ_parser::StmtKind;

    match &stmt.kind {
        StmtKind::Let {
            name: decl_name,
            initializer,
            ..
        }
        | StmtKind::Var {
            name: decl_name,
            initializer,
            ..
        } => {
            if decl_name == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&stmt.span),
                });
            }
            collect_from_expr(initializer, name, uri, locations);
        }
        StmtKind::Const {
            name: decl_name,
            initializer,
        } => {
            if decl_name == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&stmt.span),
                });
            }
            collect_from_expr(initializer, name, uri, locations);
        }
        StmtKind::ExprStmt(expr) => {
            collect_from_expr(expr, name, uri, locations);
        }
        StmtKind::Return(Some(expr)) => {
            collect_from_expr(expr, name, uri, locations);
        }
        StmtKind::Assignment { target, value, .. } => {
            collect_from_expr(target, name, uri, locations);
            collect_from_expr(value, name, uri, locations);
        }
        StmtKind::Func(func) => {
            if func.name == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&stmt.span),
                });
            }
            collect_identifier_locations(&func.body, name, uri, locations);
        }
        StmtKind::Class(class) => {
            if class.name == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&stmt.span),
                });
            }
            for method in &class.methods {
                collect_identifier_locations(&method.body, name, uri, locations);
            }
        }
        StmtKind::Trait(trait_decl) => {
            if trait_decl.name == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&stmt.span),
                });
            }
        }
        StmtKind::Enum(enum_decl) => {
            if enum_decl.name == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&stmt.span),
                });
            }
        }
        StmtKind::Block(stmts) => {
            collect_identifier_locations(stmts, name, uri, locations);
        }
        StmtKind::If {
            condition,
            then_block,
            else_branch,
        } => {
            collect_from_expr(condition, name, uri, locations);
            collect_identifier_locations(then_block, name, uri, locations);
            if let Some(branch) = else_branch {
                match branch {
                    writ_parser::ElseBranch::ElseIf(s) => {
                        collect_from_stmt(s, name, uri, locations);
                    }
                    writ_parser::ElseBranch::ElseBlock(stmts) => {
                        collect_identifier_locations(stmts, name, uri, locations);
                    }
                }
            }
        }
        StmtKind::While { condition, body } => {
            collect_from_expr(condition, name, uri, locations);
            collect_identifier_locations(body, name, uri, locations);
        }
        StmtKind::For {
            variable,
            iterable,
            body,
        } => {
            if variable == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&stmt.span),
                });
            }
            collect_from_expr(iterable, name, uri, locations);
            collect_identifier_locations(body, name, uri, locations);
        }
        StmtKind::Export(inner) => {
            collect_from_stmt(inner, name, uri, locations);
        }
        StmtKind::When { subject, arms } => {
            if let Some(expr) = subject {
                collect_from_expr(expr, name, uri, locations);
            }
            for arm in arms {
                match &arm.body {
                    writ_parser::WhenBody::Expr(expr) => {
                        collect_from_expr(expr, name, uri, locations);
                    }
                    writ_parser::WhenBody::Block(stmts) => {
                        collect_identifier_locations(stmts, name, uri, locations);
                    }
                }
            }
        }
        StmtKind::Start(expr) => {
            collect_from_expr(expr, name, uri, locations);
        }
        _ => {}
    }
}

fn collect_from_expr(
    expr: &writ_parser::Expr,
    name: &str,
    uri: &lsp_types::Uri,
    locations: &mut Vec<Location>,
) {
    use writ_parser::ExprKind;

    match &expr.kind {
        ExprKind::Identifier(ident) => {
            if ident == name {
                locations.push(Location {
                    uri: uri.clone(),
                    range: span_to_range(&expr.span),
                });
            }
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            collect_from_expr(lhs, name, uri, locations);
            collect_from_expr(rhs, name, uri, locations);
        }
        ExprKind::Unary { operand, .. } => {
            collect_from_expr(operand, name, uri, locations);
        }
        ExprKind::Grouped(inner) => {
            collect_from_expr(inner, name, uri, locations);
        }
        ExprKind::Call { callee, args } => {
            collect_from_expr(callee, name, uri, locations);
            for arg in args {
                match arg {
                    writ_parser::CallArg::Positional(expr) => {
                        collect_from_expr(expr, name, uri, locations);
                    }
                    writ_parser::CallArg::Named { value, .. } => {
                        collect_from_expr(value, name, uri, locations);
                    }
                }
            }
        }
        ExprKind::MemberAccess { object, .. } | ExprKind::SafeAccess { object, .. } => {
            collect_from_expr(object, name, uri, locations);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_from_expr(condition, name, uri, locations);
            collect_from_expr(then_expr, name, uri, locations);
            collect_from_expr(else_expr, name, uri, locations);
        }
        ExprKind::NullCoalesce { lhs, rhs }
        | ExprKind::Range {
            start: lhs,
            end: rhs,
            ..
        } => {
            collect_from_expr(lhs, name, uri, locations);
            collect_from_expr(rhs, name, uri, locations);
        }
        ExprKind::Lambda { body, .. } => match body {
            writ_parser::LambdaBody::Expr(expr) => {
                collect_from_expr(expr, name, uri, locations);
            }
            writ_parser::LambdaBody::Block(stmts) => {
                collect_identifier_locations(stmts, name, uri, locations);
            }
        },
        ExprKind::ArrayLiteral(elements) => {
            for element in elements {
                match element {
                    writ_parser::ArrayElement::Expr(expr)
                    | writ_parser::ArrayElement::Spread(expr) => {
                        collect_from_expr(expr, name, uri, locations);
                    }
                }
            }
        }
        ExprKind::DictLiteral(elements) => {
            for element in elements {
                match element {
                    writ_parser::DictElement::KeyValue { key, value } => {
                        collect_from_expr(key, name, uri, locations);
                        collect_from_expr(value, name, uri, locations);
                    }
                    writ_parser::DictElement::Spread(expr) => {
                        collect_from_expr(expr, name, uri, locations);
                    }
                }
            }
        }
        ExprKind::ErrorPropagate(inner) | ExprKind::Cast { expr: inner, .. } => {
            collect_from_expr(inner, name, uri, locations);
        }
        ExprKind::StringInterpolation(segments) => {
            for segment in segments {
                if let writ_parser::InterpolationSegment::Expression(expr) = segment {
                    collect_from_expr(expr, name, uri, locations);
                }
            }
        }
        ExprKind::Tuple(exprs) => {
            for expr in exprs {
                collect_from_expr(expr, name, uri, locations);
            }
        }
        ExprKind::Yield(Some(inner)) => {
            collect_from_expr(inner, name, uri, locations);
        }
        ExprKind::When { subject, arms } => {
            if let Some(subj) = subject {
                collect_from_expr(subj, name, uri, locations);
            }
            for arm in arms {
                if let writ_parser::WhenBody::Block(stmts) = &arm.body {
                    for stmt in stmts {
                        collect_from_stmt(stmt, name, uri, locations);
                    }
                } else if let writ_parser::WhenBody::Expr(expr) = &arm.body {
                    collect_from_expr(expr, name, uri, locations);
                }
            }
        }
        ExprKind::Index { object, index } => {
            collect_from_expr(object, name, uri, locations);
            collect_from_expr(index, name, uri, locations);
        }
        ExprKind::NamespaceAccess { .. } | ExprKind::Literal(_) | ExprKind::Yield(None) => {}
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
