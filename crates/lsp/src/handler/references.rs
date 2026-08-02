//! "Find references": every occurrence of the symbol under the cursor.
//!
//! Rides the same shared core as rename ([`GlobalState::resolve_use`] +
//! [`GlobalState::find_occurrences`]). In-file by design (the symbol's defining file); cross-file
//! references are a follow-up.

use anyhow::Result;
use lsp_types::{Location, ReferenceParams};

use crate::global_state::GlobalState;
use crate::resolver::identifier_at;
use crate::source_db::SourceDatabase;

/// Entry point for the `textDocument/references` request. Returns the declaration plus every
/// in-scope use as `Location`s (file-tagged), or `None` if the cursor isn't on a referenceable
/// identifier or the file is unknown. Never errors.
pub fn handle(state: &GlobalState, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let Some(ctx) = state.cursor_context(&uri, position) else {
        return Ok(None);
    };
    let Some(token) = identifier_at(&ctx.ast, ctx.offset) else {
        return Ok(None);
    };

    let Some(target) = state.resolve_use(&ctx.file_db, &token).into_iter().next() else {
        return Ok(None);
    };

    // In-file references: all occurrences live in the symbol's defining file.
    let def_file_db = state.source_db.file_db(target.0);
    let locations: Vec<Location> = state
        .find_occurrences(&target)
        .into_iter()
        .map(|t| Location::new(def_file_db.file_path.clone(), def_file_db.token_range(&t)))
        .collect();
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
}
