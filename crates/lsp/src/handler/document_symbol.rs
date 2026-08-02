//! Document outline / symbol support (placeholder).
//!
//! Returns no symbols yet. When implemented, walk the program AST and emit one symbol per
//! template / function / signal / variable / component with its location and kind.

use anyhow::Result;
use lsp_types::{DocumentSymbolParams, DocumentSymbolResponse};

use crate::global_state::GlobalState;

pub fn handle(
    _state: &GlobalState,
    _params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    Ok(None)
}
