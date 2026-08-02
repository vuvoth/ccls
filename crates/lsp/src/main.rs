use std::error::Error;
use std::path::PathBuf;

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
pub mod source_db;
pub mod symbol_table;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    // All logging must go to stderr — stdout is the LSP message channel.
    eprintln!("starting ccls (circom language server)");

    let (connection, io_threads) = Connection::stdio();

    let server_capabilities =
        serde_json::to_value(server_capabilities()).expect("ServerCapabilities is serializable");
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
/// `formatting` are registered as placeholders — the client routes them to the server, which
/// currently returns an empty result until each is implemented in `handler::*`. `rename` is fully
/// implemented and advertises `prepareSupport` so the client consults the server (not its own
/// textual word check) before opening the rename box — keywords/strings never become renamable.
fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        references_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(lsp_types::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        ..Default::default()
    }
}

/// Receive messages over the LSP transport and route them to `GlobalState`. Requests are answered
/// with a `Response`; notifications mutate state; responses from the client are ignored.
fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let params: InitializeParams = serde_json::from_value(params)?;

    // Capture workspace roots so `include` resolution can be confined to them (path-traversal
    // defense). Without roots the server refuses to load any include rather than read arbitrarily.
    let mut state = GlobalState::new(workspace_roots(&params));

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

/// Workspace root folders from the `initialize` handshake, in priority order: modern clients send
/// `workspace_folders`; older clients send a single `root_uri`. Each is converted from its `file:`
/// URI to a path and canonicalized (so the Vfs's pure containment check compares canonical vs
/// canonical). Non-`file:` roots and roots that can't be canonicalized are dropped.
fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    let raw: Vec<PathBuf> = if let Some(folders) = &params.workspace_folders {
        let roots: Vec<PathBuf> = folders
            .iter()
            .filter_map(|f| f.uri.to_file_path().ok())
            .collect();
        if !roots.is_empty() {
            roots
        } else {
            params
                .root_uri
                .as_ref()
                .and_then(|uri| uri.to_file_path().ok())
                .map(|root| vec![root])
                .unwrap_or_default()
        }
    } else {
        params
            .root_uri
            .as_ref()
            .and_then(|uri| uri.to_file_path().ok())
            .map(|root| vec![root])
            .unwrap_or_default()
    };
    raw.into_iter()
        .filter_map(|r| r.canonicalize().ok())
        .collect()
}
