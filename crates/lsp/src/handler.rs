//! LSP request handlers. Each feature lives in its own module and exposes a `handle` function with
//! the signature `fn handle(state: &GlobalState, params: P) -> Result<Option<R>>`. Dispatch lives in
//! [`crate::global_state::GlobalState::handle_request`].
//!
//! Implemented: `goto_definition`, `goto_implementation` (delegates to definition), `hover`,
//! `completion`, `references`, `rename` (+ `prepareRename`), `workspace_symbol`. Placeholders
//! returning `None`: `document_symbol`, `formatting`. Diagnostics are pushed via
//! `textDocument/publishDiagnostics` from the notification path (no request handler).

pub mod completion;
pub mod document_symbol;
pub mod formatting;
pub mod goto_definition;
pub mod goto_implementation;
pub mod hover;
pub mod references;
pub mod rename;
pub mod workspace_symbol;
