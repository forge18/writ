use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CompletionOptions, HoverProviderCapability, InitializeParams, OneOf, PublishDiagnosticsParams,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
        PublishDiagnostics,
    },
    request::{Completion, GotoDefinition, HoverRequest, References, Rename},
};
use serde_json::Value;

use crate::completion::handle_completion;
use crate::definition::handle_goto_definition;
use crate::document::WorldState;
use crate::hover::handle_hover;
use crate::references::handle_references;
use crate::rename::handle_rename;

/// Runs the LSP server on stdin/stdout.
pub fn run_server() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
            ..Default::default()
        }),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        rename_provider: Some(OneOf::Left(true)),
        ..Default::default()
    })?;

    let init_params = connection.initialize(server_capabilities)?;
    let _init_params: InitializeParams = serde_json::from_value(init_params)?;

    main_loop(&connection, _init_params)?;

    io_threads.join()?;
    Ok(())
}

fn main_loop(
    connection: &Connection,
    _init_params: InitializeParams,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let mut world = WorldState::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                handle_request(connection, &world, req)?;
            }
            Message::Notification(not) => {
                handle_notification(connection, &mut world, not)?;
            }
            Message::Response(_) => {}
        }
    }

    Ok(())
}

fn handle_request(
    connection: &Connection,
    world: &WorldState,
    req: Request,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    // Try each request type.
    let req = match cast_request::<Completion>(req) {
        Ok((id, params)) => {
            let result = handle_completion(world, params);
            send_response(connection, id, result)?;
            return Ok(());
        }
        Err(req) => req,
    };

    let req = match cast_request::<GotoDefinition>(req) {
        Ok((id, params)) => {
            let result = handle_goto_definition(world, params);
            send_response(connection, id, result)?;
            return Ok(());
        }
        Err(req) => req,
    };

    let req = match cast_request::<References>(req) {
        Ok((id, params)) => {
            let result = handle_references(world, params);
            send_response(connection, id, result)?;
            return Ok(());
        }
        Err(req) => req,
    };

    let req = match cast_request::<HoverRequest>(req) {
        Ok((id, params)) => {
            let result = handle_hover(world, params);
            send_response(connection, id, result)?;
            return Ok(());
        }
        Err(req) => req,
    };

    match cast_request::<Rename>(req) {
        Ok((id, params)) => {
            let result = handle_rename(world, params);
            send_response(connection, id, result)?;
        }
        Err(_req) => {
            // Unknown request type — ignored.
        }
    }

    Ok(())
}

fn handle_notification(
    connection: &Connection,
    world: &mut WorldState,
    not: Notification,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let not = match cast_notification::<DidOpenTextDocument>(not) {
        Ok(params) => {
            let uri = params.text_document.uri.clone();
            world.open_document(params.text_document.uri, params.text_document.text);
            publish_diagnostics(connection, world, &uri)?;
            return Ok(());
        }
        Err(not) => not,
    };

    let not = match cast_notification::<DidChangeTextDocument>(not) {
        Ok(params) => {
            let uri = params.text_document.uri.clone();
            // Full sync — use the last content change.
            if let Some(change) = params.content_changes.into_iter().last() {
                world.update_document(params.text_document.uri, change.text);
            }
            publish_diagnostics(connection, world, &uri)?;
            return Ok(());
        }
        Err(not) => not,
    };

    match cast_notification::<DidCloseTextDocument>(not) {
        Ok(params) => {
            let uri = &params.text_document.uri;
            world.close_document(uri);
            // Clear diagnostics for the closed file.
            let clear = PublishDiagnosticsParams {
                uri: uri.clone(),
                diagnostics: vec![],
                version: None,
            };
            let not = lsp_server::Notification::new(PublishDiagnostics::METHOD.to_string(), clear);
            connection.sender.send(Message::Notification(not))?;
        }
        Err(_not) => {
            // Unknown notification — ignored.
        }
    }

    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    world: &WorldState,
    uri: &lsp_types::Uri,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let diagnostics = world
        .get_document(uri)
        .map(|doc| doc.diagnostics.clone())
        .unwrap_or_default();

    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };

    let not = lsp_server::Notification::new(PublishDiagnostics::METHOD.to_string(), params);
    connection.sender.send(Message::Notification(not))?;
    Ok(())
}

/// Attempts to cast a request to a specific LSP request type.
/// Returns `Ok((id, params))` on success, or `Err(req)` on method mismatch.
fn cast_request<R>(req: Request) -> Result<(RequestId, R::Params), Request>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    match req.extract::<R::Params>(R::METHOD) {
        Ok(value) => Ok(value),
        Err(ExtractError::MethodMismatch(req)) => Err(req),
        Err(ExtractError::JsonError { method, error }) => {
            log::error!("Failed to deserialize {method} request: {error}");
            // Return an empty request that won't match anything.
            Err(Request {
                id: RequestId::from(0),
                method: String::new(),
                params: Value::Null,
            })
        }
    }
}

/// Attempts to cast a notification to a specific LSP notification type.
fn cast_notification<N>(not: Notification) -> Result<N::Params, Notification>
where
    N: lsp_types::notification::Notification,
    N::Params: serde::de::DeserializeOwned,
{
    match not.extract::<N::Params>(N::METHOD) {
        Ok(value) => Ok(value),
        Err(ExtractError::MethodMismatch(not)) => Err(not),
        Err(ExtractError::JsonError { method, error }) => {
            log::error!("Failed to deserialize {method} notification: {error}");
            Err(Notification {
                method: String::new(),
                params: Value::Null,
            })
        }
    }
}

fn send_response<T: serde::Serialize>(
    connection: &Connection,
    id: RequestId,
    result: Option<T>,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let result = Some(match result {
        Some(r) => serde_json::to_value(r).unwrap(),
        None => Value::Null,
    });
    let resp = Response {
        id,
        result,
        error: None,
    };
    connection.sender.send(Message::Response(resp))?;
    Ok(())
}
