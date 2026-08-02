//! Whole-document formatting support (placeholder).
//!
//! Returns no edits yet. When implemented, run a circom formatter over the document text and return
//! the resulting `TextEdit`s (e.g. by re-tokenizing and normalizing whitespace/indentation).

use anyhow::Result;
use lsp_types::{DocumentFormattingParams, TextEdit};

use crate::global_state::GlobalState;

pub fn handle(
    _state: &GlobalState,
    _params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    Ok(None)
}
