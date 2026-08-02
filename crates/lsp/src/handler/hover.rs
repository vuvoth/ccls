//! Hover: show the declaration of the symbol under the cursor.
//!
//! Rides the shared resolution core ([`GlobalState::cursor_context`] +
//! [`GlobalState::resolve_use`]). For a resolved identifier (a declared name or a usage of one) it
//! returns the symbol's kind and its declaration signature (the source text of the defining node,
//! trimmed to the header for block-bodied defs). Member-access fields (`c.x`) aren't resolved by the
//! flat resolver and yield `None` (consistent with rename/references).

use anyhow::Result;
use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind};

use crate::global_state::GlobalState;
use crate::resolver::{component_field, identifier_at, SymbolKind};
use crate::source_db::SourceDatabase;

/// Entry point for the `textDocument/hover` request. Returns `None` (no hover) when the cursor
/// isn't on a resolved identifier or the file is unknown. Never errors.
pub fn handle(state: &GlobalState, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let Some(ctx) = state.cursor_context(&uri, position) else {
        return Ok(None);
    };
    let Some(token) = identifier_at(&ctx.ast, ctx.offset) else {
        return Ok(None);
    };
    // A component member-access field (`c.x` / `T()(...).x`) resolves via type inference; everything
    // else uses the flat name resolver.
    let resolved = if component_field(&token).is_some() {
        state.resolve_member(&ctx.file_db, &token)
    } else {
        state.resolve_use(&ctx.file_db, &token)
    };
    let Some((def_id, sym)) = resolved.into_iter().next() else {
        return Ok(None);
    };

    // The declaration's source text, sliced from the defining file via `decl_range` (the whole
    // declaration — `def_range` is just the identifier, which is too narrow for a useful hover).
    let def_db = state.source_db.file_db(def_id);
    let start = u32::from(def_db.offset(sym.decl_range.start)) as usize;
    let end = u32::from(def_db.offset(sym.decl_range.end)) as usize;
    let decl = def_db.text().get(start..end).unwrap_or_default();
    let value = format!(
        "**{}** `{}`\n\n```circom\n{}\n```",
        kind_label(sym.kind),
        sym.name,
        signature(decl)
    );

    Ok(Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: Some(ctx.file_db.token_range(&token)),
    }))
}

/// The declaration header: for block-bodied defs (template/function/bus) keep only the text up to
/// `{`; otherwise the whole node text. Collapsed to one trimmed line.
fn signature(decl: &str) -> String {
    let header = decl.split('{').next().unwrap_or(decl);
    header.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Template => "template",
        SymbolKind::Function => "function",
        SymbolKind::Bus => "bus",
        SymbolKind::Signal => "signal",
        SymbolKind::Variable => "variable",
        SymbolKind::Component => "component",
        SymbolKind::Param => "parameter",
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::{Position, Url};

    use crate::file_db::{FileDB, FileId};
    use crate::global_state::GlobalState;
    use parser::token_kind::TokenKind;
    use syntax::syntax::syntax_tree;

    use super::handle;
    use lsp_types::{
        HoverContents, HoverParams, MarkupKind, TextDocumentIdentifier, TextDocumentPositionParams,
    };

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

    /// The hover markdown value for the cursor's position, or `None`.
    fn hover_value(state: &GlobalState, url: &Url, position: Position) -> Option<String> {
        handle(
            state,
            HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url.clone() },
                    position,
                },
                work_done_progress_params: Default::default(),
            },
        )
        .unwrap()
        .map(|h| match h.contents {
            HoverContents::Markup(m) if m.kind == MarkupKind::Markdown => m.value,
            _ => String::new(),
        })
    }

    /// Hovering a signal **usage** shows its declaration (`signal input a`).
    #[test]
    fn hover_signal_usage_shows_decl_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() {\n    signal input a;\n    signal output c;\n    c <== a;\n}\n";
        let url = Url::from_file_path("/tmp/h.circom").unwrap();
        let state = state_with(&url, source);

        // Cursor on the `a` usage in `c <== a` (line 4, col ~10).
        let v = hover_value(&state, &url, Position::new(4, 10)).expect("hover present");
        assert!(v.contains("**signal**"), "kind shown: {v}");
        assert!(v.contains("`a`"), "name shown: {v}");
        assert!(v.contains("signal input a"), "declaration shown: {v}");
    }

    /// Hovering a template **name** shows its header (`template Multiplier2(a, b)`), not the body.
    #[test]
    fn hover_template_name_shows_header_test() {
        let source = "pragma circom 2.0.0;\ntemplate Multiplier2(a, b) {\n    signal input a;\n    signal output c;\n}\n";
        let url = Url::from_file_path("/tmp/h2.circom").unwrap();
        let state = state_with(&url, source);

        let v = hover_value(&state, &url, Position::new(1, 12)).expect("hover present");
        assert!(v.contains("**template**"), "{v}");
        assert!(v.contains("`Multiplier2`"), "{v}");
        assert!(
            v.contains("template Multiplier2(a, b)"),
            "header, not body: {v}"
        );
        // The body must not leak into the hover.
        assert!(!v.contains("signal input a"), "body trimmed: {v}");
    }

    /// A keyword / unresolved token yields no hover.
    #[test]
    fn hover_unresolved_is_none_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() { signal input a; }\n";
        let url = Url::from_file_path("/tmp/h3.circom").unwrap();
        let state = state_with(&url, source);

        // Cursor on `template` (a keyword — identifier_at returns None).
        assert!(hover_value(&state, &url, Position::new(1, 1)).is_none());
    }

    /// Hover on an anonymous component's member field (`T()().out`) shows the signal declaration
    /// from the template (the member-resolution path), not `None`.
    #[test]
    fn hover_member_field_anonymous_test() {
        let source = "pragma circom 2.0.0;\ntemplate T() {\n    signal output out;\n    out <== 0;\n}\ntemplate Main() {\n    signal output c;\n    c <== T()().out;\n}\n";
        let url = Url::from_file_path("/tmp/hmem.circom").unwrap();
        let state = state_with(&url, source);

        // `out` occurrences: [0]=decl, [1]=usage in T, [2]=the `.out` field.
        let v = hover_value(&state, &url, position_of(source, "out", 2)).expect("hover present");
        assert!(v.contains("**signal**"), "kind shown: {v}");
        assert!(v.contains("`out`"), "name shown: {v}");
        assert!(v.contains("signal output out"), "declaration shown: {v}");
    }
}
