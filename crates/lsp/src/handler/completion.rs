//! Autocompletion support (placeholder).
//!
//! Returns no completions yet. When implemented, offer keywords (`template`, `signal`, `component`,
//! …), the templates/functions in scope, and in-scope signal/variable/component names.

use anyhow::Result;
use lsp_types::{CompletionParams, CompletionResponse};

use crate::global_state::GlobalState;

pub fn handle(
    _state: &GlobalState,
    _params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    Ok(None)
}
