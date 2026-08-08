//! Shared helpers for `ccls` unit-test modules (`#[cfg(test)]`-only).

#![cfg(test)]

use lsp_types::{Position, Url};
use parser::token_kind::TokenKind;
use syntax::node::SyntaxToken;
use syntax::tree::syntax_tree;

use crate::file_db::{FileDB, FileId};
use crate::global_state::GlobalState;

/// A `GlobalState` with one open document and no workspace roots (in-file only).
pub(crate) fn state_with(url: &Url, source: &str) -> GlobalState {
    let mut state = GlobalState::new(Vec::new());
    state.source_db.set_document(url, source.to_string());
    state
}

/// `Position` of the `occurrence`-th `Identifier` token named `name` (document order).
pub(crate) fn position_of(source: &str, name: &str, occurrence: usize) -> Position {
    token_position(
        source,
        |t| t.kind() == TokenKind::Identifier && t.text() == name,
        occurrence,
    )
    .unwrap_or_else(|| panic!("identifier token {name}#{occurrence} not found"))
}

/// `Position` of the first token of any kind whose text equals `text` (keywords / include-strings).
pub(crate) fn position_of_token(source: &str, text: &str) -> Position {
    token_position(source, |t: &SyntaxToken| t.text() == text, 0)
        .unwrap_or_else(|| panic!("token {text:?} not found"))
}

/// `Position` of the `occurrence`-th token matching `predicate`, or `None`.
fn token_position(
    source: &str,
    predicate: impl Fn(&SyntaxToken) -> bool,
    occurrence: usize,
) -> Option<Position> {
    let file = FileDB::new(FileId(0), source, Url::from_file_path("/tmp/x").unwrap());
    let node = syntax_tree(source);
    let mut count = 0;
    for t in node
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
    {
        if predicate(&t) {
            if count == occurrence {
                return Some(file.position(t.text_range().start()));
            }
            count += 1;
        }
    }
    None
}
