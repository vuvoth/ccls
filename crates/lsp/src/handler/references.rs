//! "Find references": every occurrence of the symbol under the cursor.
//!
//! Rides the same shared core as rename ([`GlobalState::resolve_use`] +
//! [`GlobalState::find_occurrences`]). In-file by design (the symbol's defining file); cross-file
//! references are a follow-up.

use anyhow::Result;
use lsp_types::{Location, ReferenceParams};

use parser::token_kind::TokenKind;

use crate::global_state::GlobalState;
use crate::resolver::token_at_offset;
use crate::source_db::SourceDatabase;

/// Entry point for the `textDocument/references` request. Returns the declaration plus every
/// in-scope use as `Location`s (file-tagged), or `None` if the cursor isn't on a referenceable
/// identifier or the file is unknown. Never errors.
pub fn handle(state: &GlobalState, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let Some(id) = state.source_db.id_for_url(&uri) else {
        return Ok(None);
    };
    let Some(ast) = state.source_db.ast(id) else {
        return Ok(None);
    };
    let file_db = state.source_db.file_db(id);

    let offset = file_db.offset(position);
    let Some(token) = token_at_offset(&ast, offset) else {
        return Ok(None);
    };
    if token.kind() != TokenKind::Identifier {
        return Ok(None);
    }

    let Some(target) = state.resolve_use(&file_db, &token).into_iter().next() else {
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
