//! Autocompletion: names visible at the cursor plus reserved keywords, and member completion
//! (`c.<signal>`) for components. Member mode detects `receiver.<partial>` by scanning the source
//! prefix, resolves the receiver's template (in-file or via include), and offers its signals.
//! Normal mode returns `is_incomplete: false` (the client filters by the typed word).

use std::collections::HashSet;

use anyhow::Result;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
};
use rowan::TextSize;

use crate::global_state::{CursorContext, GlobalState};
use crate::resolver::SymbolKind;
use crate::source_db::SourceDatabase;

/// Reserved circom keywords (mirror the lexer keywords in `token_kind.rs`). A constant list keeps
/// completion allocation-free; drift is low (keywords change rarely).
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

/// Entry point for `textDocument/completion`. Suggests in-scope body symbols + file top-level
/// names + keywords, deduped by name. `None` for an unknown file.
pub fn handle(state: &GlobalState, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let Some(ctx) = state.cursor_context(&uri, position) else {
        return Ok(None);
    };
    let table = state.source_db.symbol_table(ctx.id);

    // Member completion: cursor in a `receiver.<partial>` shape — resolve the receiver's template
    // and offer its signals. Falls through to normal completion if no such shape, not a component,
    // or template unfound.
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

/// A circom identifier byte (`[A-Za-z0-9_$]`). ASCII identifier runs ⇒ a byte check is correct and
/// stays on UTF-8 boundaries.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// If `prefix` (text up to cursor) ends in `receiver.<partial-member>`, return the receiver name
/// and its start byte offset; else `None`.
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

/// Member completion for `receiver`: resolve its component type, locate the template (in-file or
/// via include), return its signal members. `None` if not a component or template unresolved.
fn member_items(
    state: &GlobalState,
    ctx: &CursorContext,
    receiver: &str,
    recv_off: TextSize,
) -> Option<Vec<CompletionItem>> {
    // Bind the owned table so the `&str` type name outlives `resolve_template_file` / `members_of`.
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

    /// A non-component receiver (`var v; v.`) falls through to normal completion.
    #[test]
    fn member_completion_non_component_falls_through_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() { signal input a; }\ntemplate Main() {\n    var v = 0;\n    v.\n}\n";
        let url = Url::from_file_path("/tmp/n.circom").unwrap();
        let state = state_with(&url, source);

        let got = labels(&state, &url, position_after_last(source, "v."));
        // Fell through → keywords present; T's signal "a" is local to T, not visible in Main.
        assert!(
            got.contains("signal"),
            "non-component receiver falls through to normal completion"
        );
        assert!(
            !got.contains("a"),
            "T's signal must not leak into Main via a non-component receiver"
        );
    }

    /// Scope is per-template: completing inside A offers A's symbols, never B's (and vice-versa).
    #[test]
    fn completion_scope_is_per_template_test() {
        let source = "pragma circom 2.0.0;\ntemplate A() {\n    signal input aa;\n}\ntemplate B() {\n    signal input bb;\n}\n";
        let url = Url::from_file_path("/tmp/s.circom").unwrap();
        let state = state_with(&url, source);

        let in_a = labels(&state, &url, Position::new(2, 4)); // inside A's body
        let in_b = labels(&state, &url, Position::new(5, 4)); // inside B's body

        assert!(in_a.contains("aa"), "A's signal offered inside A");
        assert!(!in_a.contains("bb"), "B's signal must not leak into A");

        assert!(in_b.contains("bb"), "B's signal offered inside B");
        assert!(!in_b.contains("aa"), "A's signal must not leak into B");

        // Template/function names are file-global: B's *name* is offered inside A (you can write
        // `component x = B()` there), unlike B's *signals*.
        assert!(
            in_a.contains("B"),
            "sibling template name is visible across templates"
        );
    }

    /// Cross-file member completion: `component c = Lib();` where `Lib` is in an `include`d file —
    /// `c.` must offer `Lib`'s signals (resolved via the workspace include loader).
    #[test]
    fn member_completion_cross_file_template_test() {
        use std::fs;

        let base = std::env::temp_dir().join(format!("ccls_xfile_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(
            ws.join("lib.circom"),
            "pragma circom 2.0.0;\ntemplate Lib() {\n    signal input lin;\n    signal output lout;\n}\n",
        )
        .unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Main() {\n    component c = Lib();\n    c.\n}\n";
        let main_path = ws.join("main.circom");
        fs::write(&main_path, main_src).unwrap();

        let main_url = Url::from_file_path(&main_path).unwrap();
        // workspace root set so the `include` is loaded (path-traversal confinement).
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        let _ = state
            .source_db
            .set_document(&main_url, main_src.to_string());
        let _ = state.source_db.load_include(&main_url, "lib.circom");

        let got = labels(&state, &main_url, position_after_last(main_src, "c."));

        assert!(
            got.contains("lin"),
            "Lib's input signal offered via the include"
        );
        assert!(
            got.contains("lout"),
            "Lib's output signal offered via the include"
        );
        // Member mode returns only the template's signals — no keywords leak in.
        assert!(
            !got.contains("signal"),
            "cross-file member mode must not fall through: {got:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
