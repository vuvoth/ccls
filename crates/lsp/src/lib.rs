//! ccls — a Circom language server (library core). The `main.rs` binary is a thin [`run`] wrapper.

pub mod file_db;
pub mod global_state;
pub mod handler;
pub mod project_index;
pub mod resolver;
pub mod source_db;
pub mod symbol_table;

#[cfg(test)]
mod test_util;

use std::error::Error;
use std::path::PathBuf;

use lsp_server::{Connection, Message, Request, RequestId};
use lsp_types::notification::Notification;
use lsp_types::{
    CompletionOptions, HoverProviderCapability, ImplementationProviderCapability, InitializeParams,
    OneOf, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
};

use crate::global_state::GlobalState;

/// Run the language server over stdio (logging to stderr; stdout is the LSP channel).
pub fn run() -> Result<(), Box<dyn Error + Sync + Send>> {
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
/// `definition` and `implementation` are fully implemented; `implementation` behaves identically
/// to `definition` (Circom has no separate implementation targets). `hover`/`completion`/
/// `references`/`documentSymbol`/`formatting` are registered as placeholders — the client routes
/// them to the server, which currently returns an empty result until each is implemented in
/// `handler::*`. `rename` is fully implemented and advertises `prepareSupport` so the client
/// consults the server (not its own textual word check) before opening the rename box —
/// keywords/strings never become renamable.
fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
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
        workspace_symbol_provider: Some(OneOf::Left(true)),
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

    // Roots scope the project `.circom` walk (the basename-index source). Includes resolve
    // circom-style relative to each source file, so navigation works even with wrong/empty roots.
    let roots = workspace_roots(&params);
    let mut state = GlobalState::new(roots.clone());

    // Background the walk so `initialize` never blocks on I/O; the thread collects canonical paths
    // **and reads file content** (pure I/O — text interning stays on the main thread), and the main
    // loop interns it between messages. Same-dir includes work immediately; non-sibling ones resolve
    // once the index lands. Eager-loading all text lets workspace references/rename/symbol scan
    // every file.
    let (index_tx, index_rx) = std::sync::mpsc::channel::<Vec<(std::path::PathBuf, String)>>();
    if !roots.is_empty() {
        let walk_roots = roots.clone();
        std::thread::spawn(move || {
            let entries = crate::project_index::collect_circom_files_with_content(&walk_roots);
            let _ = index_tx.send(entries); // error ⇒ receiver gone (shutdown); ignore
        });
    }

    let mut watcher_registered = false;

    for msg in &connection.receiver {
        // Apply any completed walk results without blocking before handling this message.
        while let Ok(entries) = index_rx.try_recv() {
            state.register_indexed_paths(entries);
        }
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                if let Some(resp) = state.handle_request(req)? {
                    connection.sender.send(Message::Response(resp))?;
                }
            }
            Message::Response(_) => {}
            Message::Notification(not) => {
                // Register the watcher after `initialized` (LSP servers must not send requests
                // before it; a strict client would silently drop a pre-init registration).
                if !watcher_registered && not.method == lsp_types::notification::Initialized::METHOD
                {
                    watcher_registered = true;
                    register_watched_files_capability(&connection, &params, &roots)?;
                }
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

/// Register a `**/*.circom` watcher per root via `client/registerCapability`, if the client
/// supports dynamic registration. No-op if unsupported or no roots. Params are built as JSON to
/// avoid the proposed `GlobPattern` shape across `lsp_types` versions.
fn register_watched_files_capability(
    connection: &Connection,
    params: &InitializeParams,
    roots: &[PathBuf],
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let supported = params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|w| w.did_change_watched_files.as_ref())
        .and_then(|d| d.dynamic_registration)
        .unwrap_or(false);
    if !supported || roots.is_empty() {
        return Ok(());
    }

    let watchers: Vec<serde_json::Value> = roots
        .iter()
        .map(|root| serde_json::json!({ "globPattern": format!("{}/**/*.circom", root.display()) }))
        .collect();
    let registrations = vec![serde_json::json!({
        "id": "ccls-watched-files",
        "method": "workspace/didChangeWatchedFiles",
        "registerOptions": { "watchers": watchers }
    })];
    let req = Request {
        id: RequestId::from(String::from("ccls-register-watched-files")),
        method: String::from("client/registerCapability"),
        params: serde_json::json!({ "registrations": registrations }),
    };
    connection.sender.send(Message::Request(req))?;
    Ok(())
}
