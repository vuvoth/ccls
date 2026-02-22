use std::error::Error;

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId};
use lsp_types::{
    notification::{DidChangeTextDocument, DidOpenTextDocument, DidSaveTextDocument},
    request::{GotoDefinition, HoverRequest},
    HoverProviderCapability, OneOf, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};

use crate::global_state::{GlobalState, TextDocument};

pub mod database;
pub mod global_state;
pub mod handler;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    eprintln!("Starting Circom Language Server...");

    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        ..Default::default()
    })
    .expect("Failed to serialize server capabilities");

    let initialization_params = match connection.initialize(server_capabilities) {
        Ok(it) => it,
        Err(e) => {
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };

    main_loop(connection, initialization_params)?;
    io_threads.join()?;

    eprintln!("Circom Language Server shutdown complete");
    Ok(())
}

fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: lsp_types::InitializeParams = serde_json::from_value(params)?;
    let mut state = GlobalState::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }

                match req.method.as_str() {
                    "textDocument/definition" => {
                        handle_request::<GotoDefinition>(&connection, req, |id, params| {
                            state.goto_definition(id, params)
                        })?;
                    }
                    "textDocument/hover" => {
                        handle_request::<HoverRequest>(&connection, req, |id, params| {
                            state.hover(id, params)
                        })?;
                    }
                    _ => {}
                }
            }
            Message::Response(_) => {}
            Message::Notification(not) => match not.method.as_str() {
                "textDocument/didOpen" => {
                    if let Ok(params) = cast_notification::<DidOpenTextDocument>(&not) {
                        let _ = state.update(&TextDocument::from(params));
                    }
                }
                "textDocument/didChange" => {
                    if let Ok(params) = cast_notification::<DidChangeTextDocument>(&not) {
                        let _ = state.update(&TextDocument::from(params));
                    }
                }
                "textDocument/didSave" => {
                    if cast_notification::<DidSaveTextDocument>(&not).is_ok() {}
                }
                _ => {}
            },
        }
    }

    Ok(())
}

fn handle_request<R>(
    connection: &Connection,
    req: Request,
    handler: impl FnOnce(RequestId, R::Params) -> lsp_server::Response,
) -> Result<(), Box<dyn Error + Sync + Send>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    match req.extract(R::METHOD) {
        Ok((id, params)) => {
            let resp = handler(id, params);
            connection.sender.send(Message::Response(resp))?;
        }
        Err(ExtractError::MethodMismatch(_)) => {}
        Err(ExtractError::JsonError { method, error }) => {
            eprintln!("JSON error for {}: {}", method, error);
        }
    }
    Ok(())
}

fn cast_notification<N>(not: &Notification) -> Result<N::Params, ExtractError<()>>
where
    N: lsp_types::notification::Notification,
    N::Params: serde::de::DeserializeOwned,
{
    not.clone()
        .extract(N::METHOD)
        .map_err(|_| ExtractError::MethodMismatch(()))
}
