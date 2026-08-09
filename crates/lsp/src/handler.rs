//! LSP request handlers. Each feature lives in its own module and exposes a `handle` function with
//! the signature `fn handle(state: &GlobalState, params: P) -> Result<Option<R>>`. Dispatch lives in
//! [`crate::global_state::GlobalState::handle_request`].
//!
//! `goto_definition` is fully implemented; the rest are placeholders that return `None`/empty until
//! their logic is filled in.

pub mod completion;
pub mod document_symbol;
pub mod formatting;
pub mod goto_definition;
pub mod goto_implementation;
pub mod hover;
pub mod references;
pub mod rename;
pub mod workspace_symbol;
