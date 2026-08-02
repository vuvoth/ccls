//! Autocompletion: the names visible at the cursor plus reserved keywords.
//!
//! Rides the shared cursor prologue ([`GlobalState::cursor_context`]) and the already-cached
//! per-file [`SymbolTable`]. Returns a static `is_incomplete: false` list — the client filters by
//! the word being typed, so there's no server-side prefix extraction.

use std::collections::HashSet;

use anyhow::Result;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
};

use crate::global_state::GlobalState;
use crate::resolver::SymbolKind;
use crate::source_db::SourceDatabase;

/// Reserved circom keywords offered as completions. Mirrors the lexer keywords in
/// `token_kind.rs`; a constant list keeps completion allocation-free and drift is low (keywords
/// change rarely).
const KEYWORDS: &[&str] = &[
    "pragma",
    "include",
    "template",
    "function",
    "bus",
    "signal",
    "input",
    "output",
    "component",
    "var",
    "parallel",
    "custom",
    "extern_c",
    "custom_templates",
    "return",
    "for",
    "while",
    "if",
    "else",
    "log",
    "assert",
];

/// Entry point for the `textDocument/completion` request.
///
/// Suggests the in-scope body symbols + the file's top-level names + reserved keywords, deduped by
/// name. Returns `None` for an unknown file (no completions).
pub fn handle(state: &GlobalState, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let Some(ctx) = state.cursor_context(&uri, position) else {
        return Ok(None);
    };
    let table = state.source_db.symbol_table(ctx.id);

    let mut seen: HashSet<&str> = HashSet::new();
    let mut items: Vec<CompletionItem> = Vec::new();

    // In-scope body symbols first (params/signals/vars/components), then file top-level names.
    for sym in table
        .scope_symbols_at(ctx.offset)
        .into_iter()
        .chain(table.top_level_symbols())
    {
        if !seen.insert(sym.name.as_str()) {
            continue;
        }
        items.push(CompletionItem {
            label: sym.name.clone(),
            kind: Some(map_kind(sym.kind)),
            ..Default::default()
        });
    }

    // Reserved keywords (skip any that shadow a real symbol of the same name).
    for kw in KEYWORDS {
        if seen.insert(kw) {
            items.push(CompletionItem {
                label: (*kw).to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
        }
    }

    Ok(Some(CompletionResponse::List(CompletionList {
        is_incomplete: false,
        items,
    })))
}

/// Map a declared [`SymbolKind`] to an LSP [`CompletionItemKind`].
fn map_kind(kind: SymbolKind) -> CompletionItemKind {
    match kind {
        SymbolKind::Template | SymbolKind::Function => CompletionItemKind::FUNCTION,
        SymbolKind::Bus => CompletionItemKind::STRUCT,
        SymbolKind::Component => CompletionItemKind::FIELD,
        SymbolKind::Signal | SymbolKind::Variable | SymbolKind::Param => {
            CompletionItemKind::VARIABLE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use lsp_types::{Position, Url};

    use crate::global_state::GlobalState;

    use super::handle;
    use lsp_types::{
        CompletionParams, CompletionResponse, TextDocumentIdentifier, TextDocumentPositionParams,
    };

    fn state_with(url: &Url, source: &str) -> GlobalState {
        let mut state = GlobalState::new(Vec::new());
        state.source_db.set_document(url, source.to_string());
        state
    }

    fn labels(state: &GlobalState, url: &Url, position: Position) -> HashSet<String> {
        match handle(
            state,
            CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url.clone() },
                    position,
                },
                context: None,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .unwrap()
        .unwrap()
        {
            CompletionResponse::List(list) => list
                .items
                .into_iter()
                .map(|i| i.label)
                .collect::<HashSet<_>>(),
            CompletionResponse::Array(items) => {
                items.into_iter().map(|i| i.label).collect::<HashSet<_>>()
            }
        }
    }

    /// Cursor inside a template body → in-scope signals/params + the template name + keywords.
    #[test]
    fn completion_in_scope_includes_body_symbols_test() {
        let source =
            "pragma circom 2.0.0;\ntemplate T(a) {\n    signal input b;\n    signal output c;\n}\n";
        let url = Url::from_file_path("/tmp/c.circom").unwrap();
        let state = state_with(&url, source);

        // Position inside the body (line 2, col 4 — past `{`).
        let got = labels(&state, &url, Position::new(2, 4));

        for expected in ["a", "b", "c", "T", "signal", "template"] {
            assert!(got.contains(expected), "expected {expected:?} in {got:?}");
        }
    }

    /// Cursor at file top-level → template names present, body symbols absent.
    #[test]
    fn completion_at_top_level_excludes_body_symbols_test() {
        let source = "pragma circom 2.0.0;\ntemplate T(a) {\n    signal input b;\n}\n";
        let url = Url::from_file_path("/tmp/t.circom").unwrap();
        let state = state_with(&url, source);

        // Position on the pragma line (before any template — top-level scope).
        let got = labels(&state, &url, Position::new(0, 0));

        assert!(got.contains("T"), "template name is file-global");
        assert!(
            !got.contains("b"),
            "a body signal must not be offered at top level"
        );
        assert!(got.contains("signal"), "keywords are always offered");
    }

    /// Reserved keywords are offered.
    #[test]
    fn completion_offers_keywords_test() {
        let source = "pragma circom 2.0.0;\n";
        let url = Url::from_file_path("/tmp/k.circom").unwrap();
        let state = state_with(&url, source);

        let got = labels(&state, &url, Position::new(0, 0));
        for kw in ["signal", "template", "component", "var", "parallel"] {
            assert!(got.contains(kw), "keyword {kw:?} should be offered");
        }
    }

    /// No name is listed twice.
    #[test]
    fn completion_dedups_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() {\n    signal input a;\n}\n";
        let url = Url::from_file_path("/tmp/d.circom").unwrap();
        let state = state_with(&url, source);

        let items = match handle(
            &state,
            CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url.clone() },
                    position: Position::new(2, 4),
                },
                context: None,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .unwrap()
        .unwrap()
        {
            CompletionResponse::List(l) => l.items,
            CompletionResponse::Array(a) => a,
        };
        let mut names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before, "no duplicate completion labels");
    }
}
