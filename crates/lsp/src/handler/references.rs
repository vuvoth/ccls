//! "Find references" support (placeholder).
//!
//! Returns no references yet. When implemented, search every loaded file's semantic database for
//! declarations matching the symbol under the cursor (the inverse of goto-definition).

use anyhow::Result;
use lsp_types::{Location, ReferenceParams};

use crate::global_state::GlobalState;

pub fn handle(_state: &GlobalState, _params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    Ok(None)
}
