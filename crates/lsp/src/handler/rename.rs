//! Symbol rename: rewrite every occurrence of the symbol under the cursor to `new_name`.
//!
//! Rides the shared resolution/occurrence core ([`GlobalState::resolve_use`] +
//! [`GlobalState::find_occurrences`]) — no per-handler occurrence-walk, so it stays thin. An
//! occurrence is found by *resolving* each candidate identifier (not text-matching), which makes
//! shadowing correct. Rename is **in-file** (the symbol's defining file); cross-file rename is a
//! follow-up that needs a workspace symbol graph.

use std::collections::HashMap;

use anyhow::Result;
use lsp_types::{RenameParams, TextEdit, WorkspaceEdit};
use parser::lexer::tokenize;
use parser::token_kind::TokenKind;

use crate::global_state::GlobalState;
use crate::resolver::token_at_offset;
use crate::source_db::SourceDatabase;

/// Entry point for the `textDocument/rename` request.
///
/// Returns `None` (no edits) when the cursor isn't on a renamable `Identifier`, `new_name` isn't a
/// legal circom identifier, or the file is unknown to the server. Never errors.
pub fn handle(state: &GlobalState, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let new_name = params.new_name;

    if !is_valid_new_name(&new_name) {
        return Ok(None);
    }

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
    // Only identifiers are renamable (include-path strings route to `jump_to_lib`, not here).
    if token.kind() != TokenKind::Identifier {
        return Ok(None);
    }

    // The declaration the cursor is on. An unresolved token (e.g. a component-call field like
    // `c.x`, which the flat resolver deliberately doesn't resolve) has nothing to rename.
    let Some(target) = state.resolve_use(&file_db, &token).into_iter().next() else {
        return Ok(None);
    };

    // All occurrences live in the symbol's defining file (in-file rename; cross-file is a
    // follow-up). One edit per occurrence token.
    let def_file_db = state.source_db.file_db(target.0);
    let edits: Vec<TextEdit> = state
        .find_occurrences(&target)
        .into_iter()
        .map(|t| TextEdit {
            range: def_file_db.token_range(&t),
            new_text: new_name.clone(),
        })
        .collect();

    let changes = HashMap::from([(def_file_db.file_path.clone(), edits)]);
    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}

/// `true` if `name` is a legal circom identifier: it lexes as exactly one `Identifier` token. This
/// single check rejects reserved keywords (they lex as their own kind), digit-leading names, illegal
/// characters (lex as `Error`), empty, and multi-token names.
fn is_valid_new_name(name: &str) -> bool {
    let mut kinds = tokenize(name)
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| !k.is_trivial());
    kinds.next() == Some(TokenKind::Identifier) && kinds.next().is_none()
}

#[cfg(test)]
mod tests {
    use lsp_types::{
        Position, RenameParams, TextDocumentIdentifier, TextDocumentPositionParams, Url,
        WorkspaceEdit,
    };

    use parser::token_kind::TokenKind;
    use syntax::syntax::syntax_tree;

    use crate::file_db::{FileDB, FileId};
    use crate::global_state::GlobalState;

    use super::handle;

    fn state_with(url: &Url, source: &str) -> GlobalState {
        let mut state = GlobalState::new(Vec::new());
        state.source_db.set_document(url, source.to_string());
        state
    }

    /// LSP `Position` of the `occurrence`-th (0-indexed) `Identifier` token whose text is `name`.
    fn position_of(source: &str, name: &str, occurrence: usize) -> Position {
        let file = FileDB::new(FileId(0), source, Url::from_file_path("/tmp/x").unwrap());
        let node = syntax_tree(source);
        let mut count = 0;
        for t in node
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
        {
            if t.kind() == TokenKind::Identifier && t.text() == name {
                if count == occurrence {
                    return file.position(t.text_range().start());
                }
                count += 1;
            }
        }
        panic!("token {name}#{occurrence} not found");
    }

    fn rename(
        state: &GlobalState,
        url: &Url,
        position: Position,
        new_name: &str,
    ) -> Option<WorkspaceEdit> {
        handle(
            state,
            RenameParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url.clone() },
                    position,
                },
                work_done_progress_params: Default::default(),
                new_name: new_name.to_string(),
            },
        )
        .unwrap()
    }

    /// Renaming a signal edits both its declaration and its usage.
    #[test]
    fn rename_signal_edits_decl_and_usage_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() {\n    signal input a;\n    signal output c;\n    c <== a + 0;\n}\n";
        let url = Url::from_file_path("/tmp/rename.circom").unwrap();
        let state = state_with(&url, source);

        // Cursor on the *usage* `a` in `c <== a + 0` (the 2nd `a` token).
        let pos = position_of(source, "a", 1);
        let edits = rename(&state, &url, pos, "b")
            .unwrap()
            .changes
            .unwrap()
            .remove(&url)
            .unwrap();

        assert_eq!(edits.len(), 2, "declaration + usage");
        assert!(
            edits.iter().all(|e| e.new_text == "b"),
            "all edits use the new name"
        );
    }

    /// Renaming a symbol in one template must not touch a same-named symbol in a sibling template.
    #[test]
    fn rename_respects_scope_shadowing_test() {
        let source = "pragma circom 2.0.0;\ntemplate A() {\n    signal input a;\n    signal output o;\n    o <== a;\n}\ntemplate B() {\n    signal input a;\n    signal output o;\n    o <== a;\n}\n";
        let url = Url::from_file_path("/tmp/shadow.circom").unwrap();
        let state = state_with(&url, source);

        // Cursor on template A's declared `a` (the 0th `a` token).
        let pos = position_of(source, "a", 0);
        let edits = rename(&state, &url, pos, "aa")
            .unwrap()
            .changes
            .unwrap()
            .remove(&url)
            .unwrap();

        assert_eq!(
            edits.len(),
            2,
            "only A's declaration + A's usage; B untouched"
        );
    }

    /// A template name's declaration and its instantiation are both renamed.
    #[test]
    fn rename_template_name_and_instantiation_test() {
        let source = "pragma circom 2.0.0;\ntemplate Foo() { signal output o; o <== 0; }\ntemplate Main() { component f = Foo(); }\n";
        let url = Url::from_file_path("/tmp/tmpl.circom").unwrap();
        let state = state_with(&url, source);

        let pos = position_of(source, "Foo", 0); // the definition name
        let edits = rename(&state, &url, pos, "Bar")
            .unwrap()
            .changes
            .unwrap()
            .remove(&url)
            .unwrap();

        assert_eq!(edits.len(), 2, "definition name + instantiation");
    }

    /// An illegal new name (keyword / bad chars / empty) yields no edits rather than corrupting.
    #[test]
    fn invalid_new_name_yields_no_edits_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() { signal input a; }\n";
        let url = Url::from_file_path("/tmp/bad.circom").unwrap();
        let state = state_with(&url, source);
        let pos = position_of(source, "a", 0);

        for bad in ["signal", "1bad", "", "a b"] {
            assert!(
                rename(&state, &url, pos, bad).is_none(),
                "refuse invalid name {bad:?}"
            );
        }
    }

    /// LSP `Position` of the first token of *any* kind whose text equals `text` (for keyword /
    /// include-string cursors, which `position_of` skips because it only matches identifiers).
    fn position_of_token(source: &str, text: &str) -> Position {
        let file = FileDB::new(FileId(0), source, Url::from_file_path("/tmp/x").unwrap());
        let node = syntax_tree(source);
        for t in node
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
        {
            if t.text() == text {
                return file.position(t.text_range().start());
            }
        }
        panic!("token {text:?} not found");
    }

    /// A cursor on a keyword or an include-path string yields no edits (only identifiers rename).
    #[test]
    fn rename_refuses_non_identifier_cursor_test() {
        let source =
            "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate T() { signal input a; }\n";
        let url = Url::from_file_path("/tmp/guard.circom").unwrap();
        let state = state_with(&url, source);

        // `signal` is a keyword (no token resolves); `"lib.circom"` is a CircomString (include path).
        assert!(
            rename(&state, &url, position_of_token(source, "signal"), "x").is_none(),
            "keyword cursor is a no-op"
        );
        assert!(
            rename(
                &state,
                &url,
                position_of_token(source, "\"lib.circom\""),
                "x"
            )
            .is_none(),
            "include-path string cursor is a no-op"
        );
    }

    /// An unresolved member-access field (`c.x`) — which the flat resolver deliberately does not
    /// resolve — is a no-op, never a stray rename.
    #[test]
    fn rename_member_access_field_is_noop_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() { signal input x; signal output o; }\ntemplate Main() { component c = T(); c.x <== 0; }\n";
        let url = Url::from_file_path("/tmp/member.circom").unwrap();
        let state = state_with(&url, source);

        // The `x` in `c.x` is the 2nd `x` token (1st is T's `signal input x`); it doesn't resolve.
        let pos = position_of(source, "x", 1);
        assert!(
            rename(&state, &url, pos, "y").is_none(),
            "unresolved member-access field must not rename"
        );
    }

    /// Renaming is in-file: two documents each defining `template T()` at the same line:column must
    /// not collide — renaming in file A edits only file A, never file B. Pins the fix for the
    /// cross-file `def_range` collision the all-files search used to have.
    #[test]
    fn rename_does_not_touch_other_files_test() {
        let src = "pragma circom 2.0.0;\ntemplate T() { signal output o; o <== 0; }\n";
        let url_a = Url::from_file_path("/tmp/a.circom").unwrap();
        let url_b = Url::from_file_path("/tmp/b.circom").unwrap();
        let mut state = GlobalState::new(Vec::new());
        state.source_db.set_document(&url_a, src.to_string());
        state.source_db.set_document(&url_b, src.to_string());

        let pos = position_of(src, "T", 0);
        let changes = rename(&state, &url_a, pos, "U")
            .expect("rename produces an edit")
            .changes
            .unwrap();

        assert_eq!(changes.len(), 1, "only the cursor's file is edited");
        assert!(changes.contains_key(&url_a));
        assert!(
            !changes.contains_key(&url_b),
            "a same-named symbol in another file must not be touched"
        );
    }
}
