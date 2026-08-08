//! "Find references": every occurrence of the symbol under the cursor.
//!
//! Rides [`GlobalState::resolve_visible`] + [`GlobalState::workspace_occurrences`]. Workspace-wide
//! and resolution-based: a token is a reference only if it resolves to the target symbol, so
//! shadowing and same-name collisions across files are handled.

use anyhow::Result;
use lsp_types::{Location, ReferenceParams};

use crate::global_state::GlobalState;
use crate::resolver::identifier_at;
use crate::source_db::SourceDatabase;

/// Entry point for the `textDocument/references` request. Returns the declaration (unless
/// `context.include_declaration` is false) plus every reference across the workspace as
/// `Location`s, or `None` if the cursor isn't on a referenceable identifier or the file is
/// unknown. Never errors.
pub fn handle(state: &GlobalState, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let include_declaration = params.context.include_declaration;

    let Some(ctx) = state.cursor_context(&uri, position) else {
        return Ok(None);
    };
    let Some(token) = identifier_at(&ctx.ast, ctx.offset) else {
        return Ok(None);
    };

    let Some(target) = state
        .resolve_visible(&ctx.file_db, &token)
        .into_iter()
        .next()
    else {
        return Ok(None);
    };

    let (def_file, def_range) = (target.0, target.1.def_range);
    let mut locations: Vec<Location> = state
        .workspace_occurrences(&target, ctx.id)
        .into_iter()
        .filter(|(f, r)| include_declaration || !(*f == def_file && *r == def_range))
        .map(|(f, r)| Location::new(state.source_db.file_db(f).file_path.clone(), r))
        .collect();
    locations.sort_by(|a, b| {
        a.uri
            .cmp(&b.uri)
            .then_with(|| a.range.start.cmp(&b.range.start))
    });
    Ok(Some(locations))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Instant;

    use lsp_types::Url;
    use parser::token_kind::TokenKind;
    use rowan::ast::AstNode;

    use syntax::abstract_syntax_tree::AstCircomProgram;
    use syntax::tree::syntax_tree;

    use crate::file_db::{FileDB, FileId};
    use crate::resolver::{occurrences_in, resolve};
    use crate::symbol_table::SymbolTable;

    /// Times the `references` pipeline (parse, `SymbolTable::build`, `occurrences_in`) on a large
    /// synthetic circuit to split inherent cost (reparse) from avoidable cost (build/occurrences).
    ///
    /// `#[ignore]` keeps it out of CI; run in **release** (debug distorts the alloc-vs-traversal
    /// split): `cargo test -p ccls --release references_pipeline -- --nocapture --ignored`.
    #[test]
    #[ignore]
    fn references_pipeline_timing_test() {
        let n_signals: usize = 5_000;
        let n_usages: usize = 5_000;

        let mut src = String::from("pragma circom 2.0.0;\ntemplate Big() {\n");
        use std::fmt::Write;
        for s in 0..n_signals {
            let _ = writeln!(src, "    signal v{s};");
        }
        for _ in 0..n_usages {
            src.push_str("    v0 <== v0 + 0;\n");
        }
        src.push_str("}\n");

        let url = Url::from_file_path(Path::new("/tmp/big.circom")).unwrap();

        let t_parse = Instant::now();
        let file_db = FileDB::new(FileId(0), &src, url);
        let node = syntax_tree(&src);
        let parse_us = t_parse.elapsed().as_micros();

        let t_build = Instant::now();
        let ast = AstCircomProgram::cast(node).expect("synthetic source should parse");
        let table = SymbolTable::build(&file_db, &ast);
        let build_us = t_build.elapsed().as_micros();

        // The references target: the declaration of `v0` (the first such token in document order).
        let target_token = ast
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == TokenKind::Identifier && t.text() == "v0")
            .expect("a `v0` declaration token should exist");
        let target = resolve(&table, &target_token)
            .into_iter()
            .next()
            .expect("`v0` should resolve");

        let t_occ = Instant::now();
        let occurrences = occurrences_in(ast.syntax(), &table, &target);
        let occ_us = t_occ.elapsed().as_micros();

        eprintln!(
            "references_pipeline: signals={n_signals} usages={n_usages} | parse={parse_us}µs build={build_us}µs occurrences={occ_us}µs ({} hits)",
            occurrences.len()
        );
    }

    use lsp_types::{
        Location, Position, ReferenceContext, ReferenceParams, TextDocumentIdentifier,
        TextDocumentPositionParams,
    };

    use crate::global_state::GlobalState;
    use crate::test_util::position_of;

    /// Drive the real `textDocument/references` handler (cursor → resolve → workspace scan).
    fn refs(
        state: &GlobalState,
        url: &Url,
        position: Position,
        include_declaration: bool,
    ) -> Vec<Location> {
        super::handle(
            state,
            ReferenceParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: url.clone() },
                    position,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: ReferenceContext {
                    include_declaration,
                },
            },
        )
        .unwrap()
        .unwrap_or_default()
    }

    fn basenames(locs: &[Location]) -> Vec<String> {
        locs.iter()
            .filter_map(|l| {
                l.uri
                    .to_file_path()
                    .ok()
                    .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            })
            .collect()
    }

    /// Cross-file references: `Lib` is defined in `lib.circom`, used in `main.circom` and
    /// `other.circom`. References from a usage in main find the def in lib plus both usages — the
    /// core workspace-graph win (previously in-file only, missing both other files).
    #[test]
    fn references_span_workspace_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_refs_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_src =
            "pragma circom 2.0.0;\ntemplate Lib() {\n    signal output o;\n    o <== 0;\n}\n";
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Main() {\n    component c = Lib();\n}\n";
        let other_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Other() {\n    component d = Lib();\n}\n";
        fs::write(ws.join("lib.circom"), lib_src).unwrap();
        fs::write(ws.join("main.circom"), main_src).unwrap();
        fs::write(ws.join("other.circom"), other_src).unwrap();

        let main_url = Url::from_file_path(ws.join("main.circom").canonicalize().unwrap()).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state
            .source_db
            .set_document(&main_url, main_src.to_string());

        // Cursor on the `Lib` usage in main (`component c = Lib()`).
        let locs = refs(&state, &main_url, position_of(main_src, "Lib", 0), true);
        assert_eq!(
            locs.len(),
            3,
            "def in lib + usage in main + usage in other: {locs:?}"
        );
        let names = basenames(&locs);
        assert!(names.contains(&"lib.circom".to_string()), "{names:?}");
        assert!(names.contains(&"main.circom".to_string()), "{names:?}");
        assert!(names.contains(&"other.circom".to_string()), "{names:?}");

        let _ = fs::remove_dir_all(&base);
    }

    /// Same-name symbols in two files don't collide: `template Foo` exists independently in
    /// `a.circom` and `b.circom`, neither including the other. References of `Foo` in A find only
    /// A's occurrences; B's `Foo` is a distinct symbol (different `FileId`).
    #[test]
    fn references_same_name_distinct_files_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_refs2_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let src = "pragma circom 2.0.0;\ntemplate Foo() {\n    signal output o;\n    o <== 0;\n}\n";
        let a_path = ws.join("a.circom");
        let b_path = ws.join("b.circom");
        fs::write(&a_path, src).unwrap();
        fs::write(&b_path, src).unwrap();

        let a_url = Url::from_file_path(a_path.canonicalize().unwrap()).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state.source_db.set_document(&a_url, src.to_string());

        // Cursor on `Foo`'s definition name in A.
        let locs = refs(&state, &a_url, position_of(src, "Foo", 0), true);
        let in_a = locs.iter().filter(|l| l.uri == a_url).count();
        let names = basenames(&locs);
        assert_eq!(locs.len(), 1, "only A's `Foo` name token: {locs:?}");
        assert_eq!(in_a, 1, "the single occurrence is in A");
        assert!(
            !names.contains(&"b.circom".to_string()),
            "B's same-named Foo must not be referenced: {names:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Shadowing: when `main.circom` defines `template Foo` *and* its include `lib.circom` also
    /// defines `template Foo`, references of `Foo` from main resolve only to main's def — the
    /// include's same-named def is shadowed and never reported.
    #[test]
    fn references_shadow_include_same_name_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_refs3_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_src =
            "pragma circom 2.0.0;\ntemplate Foo() {\n    signal output o;\n    o <== 0;\n}\n";
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Foo() {\n    signal output out;\n    out <== 0;\n}\n";
        fs::write(ws.join("lib.circom"), lib_src).unwrap();
        fs::write(ws.join("main.circom"), main_src).unwrap();

        let main_url = Url::from_file_path(ws.join("main.circom").canonicalize().unwrap()).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state
            .source_db
            .set_document(&main_url, main_src.to_string());

        // Cursor on main's `Foo` definition name (occurrence 0).
        let locs = refs(&state, &main_url, position_of(main_src, "Foo", 0), true);
        let names = basenames(&locs);
        assert!(
            names.iter().all(|n| n == "main.circom"),
            "shadowed include's Foo must not appear: {names:?}"
        );
        assert!(
            !names.contains(&"lib.circom".to_string()),
            "lib's Foo is shadowed by main's: {names:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `context.include_declaration = false` omits the declaration range (the def in lib), keeping
    /// only the usages.
    #[test]
    fn references_exclude_declaration_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_refs4_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_src =
            "pragma circom 2.0.0;\ntemplate Lib() {\n    signal output o;\n    o <== 0;\n}\n";
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Main() {\n    component c = Lib();\n}\n";
        fs::write(ws.join("lib.circom"), lib_src).unwrap();
        fs::write(ws.join("main.circom"), main_src).unwrap();

        let main_url = Url::from_file_path(ws.join("main.circom").canonicalize().unwrap()).unwrap();
        let lib_url = Url::from_file_path(ws.join("lib.circom").canonicalize().unwrap()).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state
            .source_db
            .set_document(&main_url, main_src.to_string());

        let with_decl = refs(&state, &main_url, position_of(main_src, "Lib", 0), true);
        let no_decl = refs(&state, &main_url, position_of(main_src, "Lib", 0), false);
        assert_eq!(with_decl.len(), 2, "decl in lib + usage in main");
        assert_eq!(no_decl.len(), 1, "declaration excluded");
        assert!(
            no_decl.iter().all(|l| l.uri != lib_url),
            "the lib declaration is excluded when include_declaration=false"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// A **function** (non-component top-level symbol) defined in an include and called in the
    /// includer's body. This is the exact gap [`GlobalState::resolve_visible`] closes: the old gated
    /// `resolve_use` only crossed files for component decls/calls, so a `helper(a)` call inside a
    /// template body never resolved across the include. References from the call find the def in lib
    /// plus the call in main.
    #[test]
    fn references_cross_file_function_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_refs_fn_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_src = "pragma circom 2.0.0;\nfunction helper(x) {\n    return x;\n}\n";
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Main() {\n    signal input a;\n    signal output c;\n    c <== helper(a);\n}\n";
        fs::write(ws.join("lib.circom"), lib_src).unwrap();
        fs::write(ws.join("main.circom"), main_src).unwrap();

        let lib_url = Url::from_file_path(ws.join("lib.circom").canonicalize().unwrap()).unwrap();
        let main_url = Url::from_file_path(ws.join("main.circom").canonicalize().unwrap()).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state
            .source_db
            .set_document(&main_url, main_src.to_string());

        // Cursor on `helper(` call in main.
        let locs = refs(&state, &main_url, position_of(main_src, "helper", 0), true);
        assert_eq!(locs.len(), 2, "def in lib + call in main: {locs:?}");
        let names = basenames(&locs);
        assert!(names.contains(&"lib.circom".to_string()));
        assert!(names.contains(&"main.circom".to_string()));
        let _ = (lib_url, fs::remove_dir_all(&base));
    }

    /// Real circomlib scenario: references for `Num2Bits` in `bitify.circom` (which includes
    /// `comparators.circom` + `aliascheck.circom`). `Num2Bits` is defined and instantiated in the
    /// same file → 2 in-file occurrences. Also covers the exit-101 panic fix: `comparators.circom`
    /// carries a multi-byte box-drawing char, and an `IsZero` reference (defined in comparators,
    /// used in bitify) computes a range on that multi-byte file via `position()`.
    #[test]
    fn bitify_num2bits_references_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_bitify_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();

        // `─` (U+2500) in the header → multi-byte content that panicked `position()` before the fix.
        let comparators_src = "/* ─── comparators.circom ─── */\npragma circom 2.0.0;\n\ntemplate IsZero() {\n    signal input in;\n    signal output out;\n    out <== 0;\n}\n";
        let aliascheck_src =
            "pragma circom 2.0.0;\n\ntemplate AliasCheck() {\n    signal input in[254];\n}\n";
        let bitify_src = r#"pragma circom 2.0.0;

include "comparators.circom";
include "aliascheck.circom";


template Num2Bits(n) {
    signal input in;
    signal output out[n];
    var lc1=0;

    var e2=1;
    for (var i = 0; i<n; i++) {
        out[i] <-- (in >> i) & 1;
        out[i] * (out[i] -1 ) === 0;
        lc1 += out[i] * e2;
        e2 = e2+e2;
    }

    lc1 === in;
}

template Num2Bits_strict() {
    signal input in;
    signal output out[254];

    component aliasCheck = AliasCheck();
    component n2b = Num2Bits(254);
    in ==> n2b.in;

    for (var i=0; i<254; i++) {
        n2b.out[i] ==> out[i];
        n2b.out[i] ==> aliasCheck.in[i];
    }
}

template Num2BitsNeg(n) {
    signal input in;
    signal output out[n];
    var lc1=0;

    component isZero;

    isZero = IsZero();

    var neg = n == 0 ? 0 : 2**n - in;

    for (var i = 0; i<n; i++) {
        out[i] <-- (neg >> i) & 1;
        out[i] * (out[i] -1 ) === 0;
        lc1 += out[i] * 2**i;
    }

    in ==> isZero.in;



    lc1 + isZero.out * 2**n === 2**n - in;
}
"#;
        fs::write(ws.join("comparators.circom"), comparators_src).unwrap();
        fs::write(ws.join("aliascheck.circom"), aliascheck_src).unwrap();
        fs::write(ws.join("bitify.circom"), bitify_src).unwrap();

        let bitify_url =
            Url::from_file_path(ws.join("bitify.circom").canonicalize().unwrap()).unwrap();
        let comparators_url =
            Url::from_file_path(ws.join("comparators.circom").canonicalize().unwrap()).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state
            .source_db
            .set_document(&bitify_url, bitify_src.to_string());

        // References of `Num2Bits` from its definition: def name + the `Num2Bits(254)` instantiation.
        let num2bits = refs(
            &state,
            &bitify_url,
            position_of(bitify_src, "Num2Bits", 0),
            true,
        );
        assert_eq!(num2bits.len(), 2, "def + instantiation: {num2bits:?}");
        assert!(
            num2bits.iter().all(|l| l.uri == bitify_url),
            "both occurrences are in bitify.circom"
        );

        // References of `IsZero` from its usage in bitify: def in comparators + usage in bitify.
        // Computing comparators' range exercises `position()` on the multi-byte file (panic fix).
        let iszero = refs(
            &state,
            &bitify_url,
            position_of(bitify_src, "IsZero", 0),
            true,
        );
        assert_eq!(
            iszero.len(),
            2,
            "def in comparators + usage in bitify: {iszero:?}"
        );
        let names = basenames(&iszero);
        assert!(
            names.contains(&"comparators.circom".to_string()),
            "{names:?}"
        );
        assert!(names.contains(&"bitify.circom".to_string()), "{names:?}");
        let _ = (comparators_url, fs::remove_dir_all(&base));
    }
}
