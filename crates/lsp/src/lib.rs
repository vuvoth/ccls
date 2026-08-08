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
use lsp_types::{
    CompletionOptions, HoverProviderCapability, InitializeParams, OneOf, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind,
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
    let roots = workspace_roots(&params);
    let mut state = GlobalState::new(roots.clone());

    // Background the workspace walk so `initialize` never blocks on I/O (a large `node_modules` can
    // take seconds). The thread only does the I/O-heavy `collect_circom_files`; it hands the
    // canonical paths back on a side channel, and the main loop registers them between messages.
    // The server is responsive immediately; non-sibling includes resolve once the index lands, and
    // same-dir includes work from the start (never gated on the index).
    let (index_tx, index_rx) = std::sync::mpsc::channel::<Vec<std::path::PathBuf>>();
    if !roots.is_empty() {
        let walk_roots = roots.clone();
        std::thread::spawn(move || {
            let paths = crate::project_index::collect_circom_files(&walk_roots);
            // The only error is the receiver being gone (server shutting down); nothing to do then.
            let _ = index_tx.send(paths);
        });
    }

    // If the client supports it, register a `**/*.circom` watcher per root so created/deleted
    // files keep the index fresh without a full re-walk. Best-effort: a missing reply or
    // unsupported client just leaves the index stale at the file level (refreshed on workspace
    // folder changes); the `Message::Response(_)` no-op arm below absorbs the registration reply.
    register_watched_files_capability(&connection, &params, &roots)?;

    for msg in &connection.receiver {
        // Apply any completed background-walk results without blocking, before handling this
        // message. `try_recv` returns immediately when nothing is ready yet.
        while let Ok(paths) = index_rx.try_recv() {
            state.register_indexed_paths(paths);
        }
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

/// If the client advertised `workspace.didChangeWatchedFiles` dynamic-registration support,
/// register one `**/*.circom` file watcher per workspace root via a `client/registerCapability`
/// request. Best-effort: no-op if unsupported or if there are no roots. The request is sent on the
/// `connection.sender`; its (empty) reply hits the `Message::Response(_)` no-op arm in
/// [`main_loop`]. We build the params as JSON to stay independent of the proposed
/// `GlobPattern`/`RelativePattern` shape across `lsp_types` versions — the wire format is stable.
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
