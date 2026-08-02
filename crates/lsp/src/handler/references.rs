//! "Find references": every occurrence of the symbol under the cursor.
//!
//! Rides the same shared core as rename ([`GlobalState::resolve_use`] +
//! [`GlobalState::find_occurrences`]). In-file by design (the symbol's defining file); cross-file
//! references are a follow-up.

use anyhow::Result;
use lsp_types::{Location, ReferenceParams};

use crate::global_state::GlobalState;
use crate::resolver::identifier_at;
use crate::source_db::SourceDatabase;

/// Entry point for the `textDocument/references` request. Returns the declaration plus every
/// in-scope use as `Location`s (file-tagged), or `None` if the cursor isn't on a referenceable
/// identifier or the file is unknown. Never errors.
pub fn handle(state: &GlobalState, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let Some(ctx) = state.cursor_context(&uri, position) else {
        return Ok(None);
    };
    let Some(token) = identifier_at(&ctx.ast, ctx.offset) else {
        return Ok(None);
    };

    let Some(target) = state.resolve_use(&ctx.file_db, &token).into_iter().next() else {
        return Ok(None);
    };

    // In-file references: all occurrences live in the symbol's defining file.
    let def_file_db = state.source_db.file_db(target.0);
    let locations: Vec<Location> = state
        .find_occurrences(&target)
        .into_iter()
        .map(|t| Location::new(def_file_db.file_path.clone(), def_file_db.token_range(&t)))
        .collect();
    Ok(Some(locations))
}
