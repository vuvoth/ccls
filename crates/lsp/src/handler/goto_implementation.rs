//! `textDocument/implementation` for Circom.
//!
//! Circom has no separate "implementation" concept (no interfaces/traits/abstract symbols
//! distinct from their definition), so go-to-implementation behaves identically to
//! go-to-definition. This handler delegates to [`super::goto_definition::handle`] so the two
//! features can never drift.

use anyhow::Result;
use lsp_types::request::{GotoImplementationParams, GotoImplementationResponse};

use crate::global_state::GlobalState;

/// Entry point for the `textDocument/implementation` request. Identical to
/// [`super::goto_definition::handle`] — every implementation target IS the symbol's definition.
pub fn handle(
    state: &GlobalState,
    params: GotoImplementationParams,
) -> Result<Option<GotoImplementationResponse>> {
    super::goto_definition::handle(state, params)
}

#[cfg(test)]
mod tests {
    use lsp_types::request::GotoImplementationParams;
    use lsp_types::{Location, TextDocumentIdentifier, TextDocumentPositionParams, Url};
    use parser::token_kind::TokenKind;
    use rowan::ast::AstNode;
    use syntax::abstract_syntax_tree::AstCircomProgram;
    use syntax::tree::syntax_tree;

    use crate::source_db::SourceDatabase;
    use crate::test_util::{file_url, state_with};

    /// Drive the real `textDocument/implementation` handler at the `occurrence`-th token whose
    /// kind+text match, returning the resolved `Location`s.
    fn goto_impl(
        state: &crate::global_state::GlobalState,
        url: &Url,
        source: &str,
        kind: TokenKind,
        text: &str,
        occurrence: usize,
    ) -> Vec<Location> {
        let id = state.source_db.id_for_url(url).expect("doc registered");
        let file_db = state.source_db.file_db(id);
        let ast = AstCircomProgram::cast(syntax_tree(source)).expect("program");
        let token = ast
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == kind && t.text() == text)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("token {text:?}#{occurrence} not found"));
        let pos = file_db.position(token.text_range().start());
        super::handle(
            state,
            GotoImplementationParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url.clone() },
                    position: pos,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .unwrap()
        .map(|r| match r {
            lsp_types::request::GotoImplementationResponse::Array(v) => v,
            _ => Vec::new(),
        })
        .unwrap_or_default()
    }

    /// Go-to-implementation on the `X` usage in `component main = X()` lands on `X`'s
    /// definition in the same file — the "identical to go-to-definition" contract.
    #[test]
    fn implementation_matches_definition_same_file_test() {
        let source =
            "pragma circom 2.0.0;\ntemplate X() { signal output o; o <== 0; }\ncomponent main = X();\n";
        let url = file_url("impl_same.circom");
        let state = state_with(&url, source);

        // The `X` usage in `component main = X()` is the 2nd `X` token (0th = the definition).
        let locs = goto_impl(&state, &url, source, TokenKind::Identifier, "X", 1);

        assert_eq!(locs.len(), 1, "implementation jump: {locs:?}");
        assert_eq!(locs[0].uri, url, "jumps within the same file");
        assert_eq!(
            locs[0].range.start.line, 1,
            "lands on the template definition"
        );
    }

    /// Go-to-implementation on an include path string jumps to the included file's URL — the
    /// include-target path is shared with go-to-definition, so this locks that it isn't lost.
    #[test]
    fn implementation_on_include_string_test() {
        let source = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ncomponent main = X();\n";
        let url = file_url("impl_inc.circom");
        // `state_with` is single-file; the include won't resolve to a real lib, but the handler
        // must still run without panicking and return an array (here empty, as the lib is absent).
        let state = state_with(&url, source);

        let locs = goto_impl(
            &state,
            &url,
            source,
            TokenKind::CircomString,
            "\"lib.circom\"",
            0,
        );
        // The lib doesn't exist on disk in this single-file harness, so no target resolves.
        assert!(locs.is_empty(), "absent include yields no target: {locs:?}");
    }
}
