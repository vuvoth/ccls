use std::error::Error;

use lsp_server::{Connection, Message};
use lsp_types::{
    CompletionOptions, HoverProviderCapability, InitializeParams, OneOf, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
};

use crate::global_state::GlobalState;

pub mod file_db;
pub mod global_state;
pub mod handler;
pub mod resolver;
pub mod semantic;
pub mod source_db;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    // All logging must go to stderr — stdout is the LSP message channel.
    eprintln!("starting ccls (circom language server)");

    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(server_capabilities()).unwrap();
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

    eprintln!("shutting down server");
    Ok(())
}

/// Advertise the LSP features this server handles.
///
/// `definition` is fully implemented; `hover`/`completion`/`references`/`documentSymbol`/
/// `formatting`/`rename` are registered as placeholders — the client routes them to the server,
/// which currently returns an empty result until each is implemented in `handler::*`.
fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions::default()),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Left(true)),
        ..Default::default()
    }
}

/// Receive messages over the LSP transport and route them to `GlobalState`. Requests are answered
/// with a `Response`; notifications mutate state; responses from the client are ignored.
fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params)?;

    let mut state = GlobalState::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                // The `shutdown` request is handled by the transport itself.
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                if let Some(resp) = state.handle_request(req)? {
                    connection.sender.send(Message::Response(resp))?;
                }
            }
            Message::Response(_) => {}
            Message::Notification(not) => {
                state.handle_notification(not)?;
            }
        }
    }
    Ok(())
}
