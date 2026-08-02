//! Autocompletion: the names visible at the cursor plus reserved keywords, and **member
//! completion** (`c.<signal>`) for component instances.
//!
//! Rides the shared cursor prologue ([`GlobalState::cursor_context`]) and the already-cached
//! per-file [`SymbolTable`]. Member mode detects a `receiver.<partial>` shape by scanning the source
//! prefix, resolves the receiver's instantiated template (in-file or via include), and offers that
//! template's signals. Normal mode returns a static `is_incomplete: false` list — the client filters
//! by the word being typed, so there's no server-side prefix extraction.

use std::collections::HashSet;

use anyhow::Result;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
};
use rowan::TextSize;

use crate::global_state::{CursorContext, GlobalState};
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

    // Member completion: cursor sits in a `receiver.<partial>` shape. Resolve the receiver's
    // instantiated template and offer its signals. Falls through to normal completion if there's no
    // such shape, the receiver isn't a component, or its template can't be found.
    let prefix = ctx.file_db.text();
    let byte = u32::from(ctx.offset) as usize;
    let prefix = prefix.get(..byte).unwrap_or(prefix);
    if let Some((receiver, recv_off)) = parse_member_receiver(prefix) {
        if let Some(items) = member_items(state, &ctx, receiver, recv_off) {
            return Ok(Some(CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items,
            })));
        }
    }

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

/// A circom identifier byte (`[A-Za-z0-9_$]`). Member detection scans ASCII identifier runs, so a
/// byte check is correct (identifiers are ASCII) and stays on UTF-8 char boundaries.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// If the source `prefix` (text up to the cursor) ends in a `receiver.<partial-member>` shape,
/// return the receiver name and its starting byte offset (for scope lookup). `None` otherwise.
fn parse_member_receiver(prefix: &str) -> Option<(&str, TextSize)> {
    let bytes = prefix.as_bytes();
    // skip a trailing partial member (identifier chars), possibly empty (cursor right after `.`)
    let mut cur = bytes.len();
    while cur > 0 && is_ident_byte(bytes[cur - 1]) {
        cur -= 1;
    }
    // require a `.` immediately before the (possibly empty) member
    if cur == 0 || bytes[cur - 1] != b'.' {
        return None;
    }
    let end = cur - 1; // the dot
    let mut start = end;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None; // no receiver identifier before the dot
    }
    let receiver = std::str::from_utf8(&bytes[start..end]).ok()?;
    Some((receiver, TextSize::from(start as u32)))
}

/// Member completion items for `receiver`: resolve its component type, locate the instantiated
/// template (in-file or via include), and return its signal members. `None` if the receiver isn't a
/// component or its template can't be resolved.
fn member_items(
    state: &GlobalState,
    ctx: &CursorContext,
    receiver: &str,
    recv_off: TextSize,
) -> Option<Vec<CompletionItem>> {
    // `symbol_table` returns an owned table; bind it so the `&str` type name (a borrow of it) lives
    // long enough for `resolve_template_file` / `members_of` below.
    let table = state.source_db.symbol_table(ctx.id);
    let ty = table.component_type_at(recv_off, receiver)?;
    let template_file = state.resolve_template_file(&ctx.file_db, ty)?;
    Some(
        state
            .source_db
            .symbol_table(template_file)
            .members_of(ty)
            .into_iter()
            .map(|s| CompletionItem {
                label: s.name.clone(),
                kind: Some(CompletionItemKind::FIELD),
                ..Default::default()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use lsp_types::{Position, Url};
    use rowan::TextSize;

    use crate::file_db::{FileDB, FileId};
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

    /// Position at the byte offset just past the last occurrence of `needle`.
    fn position_after_last(source: &str, needle: &str) -> Position {
        let file = FileDB::new(FileId(0), source, Url::from_file_path("/tmp/x").unwrap());
        let idx = source.rfind(needle).unwrap_or(0) + needle.len();
        file.position(TextSize::from(idx as u32))
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

    /// `c.` offers the instantiated template's signals (and only those — member mode).
    #[test]
    fn member_completion_offers_template_signals_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() {\n    signal input a;\n    signal input b;\n    signal output c;\n}\ntemplate Main() {\n    component m = T();\n    m.\n}\n";
        let url = Url::from_file_path("/tmp/m.circom").unwrap();
        let state = state_with(&url, source);

        // Cursor right after the `m.` member-access dot.
        let got = labels(&state, &url, position_after_last(source, "m."));

        for signal in ["a", "b", "c"] {
            assert!(
                got.contains(signal),
                "template signal {signal:?} should be offered"
            );
        }
        // Member mode returns ONLY the template's signals — no keywords, no template name.
        assert!(
            !got.contains("signal") && !got.contains("template") && !got.contains("T"),
            "member mode must not fall through to keyword/global completion: {got:?}"
        );
    }

    /// A receiver that isn't a component (`var v; v.`) falls through to normal completion (no
    /// member items) — proves member mode doesn't fire for non-components.
    #[test]
    fn member_completion_non_component_falls_through_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() { signal input a; }\ntemplate Main() {\n    var v = 0;\n    v.\n}\n";
        let url = Url::from_file_path("/tmp/n.circom").unwrap();
        let state = state_with(&url, source);

        let got = labels(&state, &url, position_after_last(source, "v."));
        // Fell through to normal completion → keywords present, T's signal "a" absent (a is local to
        // T's body, not visible in Main).
        assert!(
            got.contains("signal"),
            "non-component receiver falls through to normal completion"
        );
        assert!(
            !got.contains("a"),
            "T's signal must not leak into Main via a non-component receiver"
        );
    }
}
