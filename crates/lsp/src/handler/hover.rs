//! Hover support (placeholder).
//!
//! Returns no hover information yet. When implemented, resolve the symbol under the cursor
//! (template / function / signal / variable / component) and return its kind, declared type, and
//! any documentation.

use anyhow::Result;
use lsp_types::{Hover, HoverParams};

use crate::global_state::GlobalState;

pub fn handle(_state: &GlobalState, _params: HoverParams) -> Result<Option<Hover>> {
    Ok(None)
}
