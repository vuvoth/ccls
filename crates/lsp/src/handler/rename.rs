//! Symbol rename support (placeholder).
//!
//! Returns no edits yet. When implemented, find every occurrence of the symbol under the cursor
//! across all loaded files (declarations + usages) and produce a `WorkspaceEdit` mapping each file
//! URI to its replacement `TextEdit`s.

use anyhow::Result;
use lsp_types::{RenameParams, WorkspaceEdit};

use crate::global_state::GlobalState;

pub fn handle(_state: &GlobalState, _params: RenameParams) -> Result<Option<WorkspaceEdit>> {
    Ok(None)
}
