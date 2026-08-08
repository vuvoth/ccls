use lsp_types::{Location, Range, Url};
use rowan::ast::AstNode;

use syntax::abstract_syntax_tree::AstInclude;
use syntax::node::SyntaxToken;
use vfs::Vfs;

use crate::file_db::FileDB;
use crate::global_state::GlobalState;
use crate::resolver::{token_ancestors, token_at_offset};

use anyhow::Result;
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse};

/// Entry point for the `textDocument/definition` request.
///
/// Resolves the token under the cursor, then runs the (possibly cross-file) definition lookup. If
/// the file is unknown to the server or no token is under the cursor, returns `None`.
pub fn handle(
    state: &GlobalState,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // Shared open-document prologue. Goto-definition keeps `token_at_offset` (not `identifier_at`)
    // because an include-path `CircomString` is also a valid jump target.
    let Some(ctx) = state.cursor_context(&uri, position) else {
        return Ok(None);
    };
    let locations = match token_at_offset(&ctx.ast, ctx.offset) {
        Some(token) => state.lookup_definition(&ctx.file_db, &token),
        None => Vec::new(),
    };
    Ok(Some(GotoDefinitionResponse::Array(locations)))
}

// If `token` is an include path (`include "lib.circom";`), jump to that file's URL. Routed here
// (not the resolver) because a `CircomString` carries a path, not a symbol name. Same resolution
// order as `load_include` so the jump target and the load always agree.
pub fn include_target_location(file_db: &FileDB, token: &SyntaxToken, vfs: &Vfs) -> Vec<Location> {
    let Some(include_stmt) = token_ancestors(token).find_map(AstInclude::cast) else {
        return Vec::new();
    };
    let Some(include_path) = include_stmt.lib() else {
        return Vec::new();
    };
    let rel = include_path.value();
    let path = file_db.get_path();
    let Some(parent_dir) = path.parent() else {
        return Vec::new();
    };
    let lib_path = parent_dir.join(&rel);

    // Same-dir: canonicalize + confine (defense-in-depth — `load_include` refuses escapes; mirror it
    // so goto-def never surfaces an unreachable path).
    if let Ok(canon) = lib_path.canonicalize() {
        if vfs.is_confined(&canon) {
            if let Some(vpath) = vfs::VfsPath::from_abs_path(&lib_path) {
                if let Ok(lib_url) = Url::from_file_path(vpath.as_path()) {
                    return vec![Location::new(lib_url, Range::default())];
                }
            }
        }
    }

    // Same-dir miss → basename fallback. `parent` is the includer file (matching `load_include`) so
    // `find_include` ranks identically. Canonicalize + confine the winner too (it may have been
    // deleted between the walk and this jump).
    let Some(parent_vpath) = vfs::VfsPath::from_abs_path(&path) else {
        return Vec::new();
    };
    let Some(winner) = vfs.find_include(&parent_vpath, &rel) else {
        return Vec::new();
    };
    let Some(winner_path) = vfs.path(winner) else {
        return Vec::new();
    };
    let Ok(canon) = winner_path.as_path().canonicalize() else {
        return Vec::new();
    };
    if !vfs.is_confined(&canon) {
        return Vec::new();
    }
    let Ok(lib_url) = Url::from_file_path(winner_path.as_path()) else {
        return Vec::new();
    };
    vec![Location::new(lib_url, Range::default())]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{Location, Url};
    use rowan::ast::AstNode;
    use syntax::{
        abstract_syntax_tree::{AstCircomProgram, AstInputSignalDecl, AstTemplateDef},
        tree::syntax_tree,
    };

    use crate::file_db::FileDB;
    use crate::global_state::GlobalState;
    use crate::source_db::SourceDatabase;
    use crate::test_util::state_with;
    use parser::token_kind::TokenKind;

    use super::token_at_offset;

    fn get_source_from_path(file_path: &str) -> String {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let full_path = format!("{}{}", crate_path, file_path);
        std::fs::read_to_string(&full_path).expect(&full_path)
    }

    /// Replaces the legacy `lookup_node_wrap_token(TokenKind::TemplateDef, &token)` call with typed
    /// ancestor navigation. The resolved node (and hence the insta snapshot) is identical — only the
    /// navigation primitive changes from a kind-string walk to a typed cast.
    #[test]
    fn goto_decl_test() {
        let file_path = "/src/test_files/handler/templates.circom";
        let source = get_source_from_path(file_path);
        let file_db = FileDB::new(
            vfs::FileId(0),
            &source,
            Url::from_file_path(Path::new("/tmp")).unwrap(),
        );

        let syntax_node = syntax_tree(&source);

        if let Some(program_ast) = AstCircomProgram::cast(syntax_node) {
            let inputs = program_ast.template_list()[0]
                .body()
                .unwrap()
                .statement_list()
                .unwrap()
                .find_children::<AstInputSignalDecl>();
            let signal_name = inputs[0].signal_identifier().unwrap().name().unwrap();

            let signal_offset = signal_name.syntax().text_range().start();

            if let Some(token) = token_at_offset(
                &program_ast,
                file_db.offset(file_db.position(signal_offset)),
            ) {
                let template_def = super::token_ancestors(&token).find_map(AstTemplateDef::cast);

                let node_text = match template_def {
                    None => "None".to_string(),
                    Some(def) => format!("{}", def.syntax()),
                };

                insta::assert_snapshot!("test_lookup_node_wrap_token", node_text);
            }
        }
    }

    #[test]
    fn url_test() {
        let url = Url::from_file_path(Path::new("/hello/abc.tx"));
        let binding = url.unwrap();
        let path = binding.path();
        let parent = Path::new(path).parent().unwrap().to_str().unwrap();

        assert_eq!("/hello", parent);
    }

    /// `lookup_definition` for the `occurrence`-th `Identifier` token named `name`, using the db's
    /// own `FileDB` (so `origin.file_id` matches the interned id `resolve_use` queries).
    fn jump(
        state: &GlobalState,
        url: &Url,
        source: &str,
        name: &str,
        occurrence: usize,
    ) -> Vec<Location> {
        let id = state
            .source_db
            .id_for_url(url)
            .expect("document registered");
        let file_db = state.source_db.file_db(id);
        let ast = AstCircomProgram::cast(syntax_tree(source)).expect("parses to a program");
        let token = ast
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == TokenKind::Identifier && t.text() == name)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("token {name}#{occurrence} not found"));
        state.lookup_definition(&file_db, &token)
    }

    /// Faithful replica of the `textDocument/definition` handler: `cursor_context` (the real gate
    /// the editor hits) → `token_at_offset` → `lookup_definition`, driven by the position of the
    /// `occurrence`-th `Identifier` named `name`.
    fn goto_at(
        state: &GlobalState,
        url: &Url,
        source: &str,
        name: &str,
        occurrence: usize,
    ) -> Vec<Location> {
        let id = state
            .source_db
            .id_for_url(url)
            .expect("document registered");
        let file_db = state.source_db.file_db(id);
        let ast = AstCircomProgram::cast(syntax_tree(source)).expect("parses to a program");
        let token = ast
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == TokenKind::Identifier && t.text() == name)
            .nth(occurrence)
            .unwrap_or_else(|| panic!("token {name}#{occurrence} not found"));
        let Some(ctx) = state.cursor_context(url, file_db.position(token.text_range().start()))
        else {
            return Vec::new();
        };
        let Some(tok) = token_at_offset(&ctx.ast, ctx.offset) else {
            return Vec::new();
        };
        state.lookup_definition(&ctx.file_db, &tok)
    }

    /// Goto-definition from the template reference inside `component main = X()` resolves to `X`'s
    /// definition **in the same file** (the in-file `lookup_top_level` path; unaffected by the gate).
    #[test]
    fn main_component_same_file_jump_test() {
        let source = "pragma circom 2.0.0;\ntemplate X() { signal output o; o <== 0; }\ncomponent main = X();\n";
        let url = Url::from_file_path("/tmp/mc_same.circom").unwrap();
        let state = state_with(&url, source);

        // The `X` usage in `component main = X()` is the 2nd `X` token (0th = the definition).
        let locs = jump(&state, &url, source, "X", 1);

        assert_eq!(locs.len(), 1, "same-file main-component jump: {locs:?}");
        assert_eq!(locs[0].uri, url, "jumps within the same file");
        // `def_range` is the template name token, on line 2 (0-indexed 1).
        assert_eq!(
            locs[0].range.start.line, 1,
            "lands on the template definition"
        );
    }

    /// Goto-definition from the canonical `component main = Lib()` entry point must jump across the
    /// `include` to `Lib`'s definition — the case the old `is_component_use` gate missed because
    /// `MainComponent` is a distinct node kind from `ComponentDecl`/`ComponentCall`.
    #[test]
    fn main_component_cross_file_jump_test() {
        use std::fs;

        let base = std::env::temp_dir().join(format!("ccls_mc_xfile_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(
            ws.join("lib.circom"),
            "pragma circom 2.0.0;\ntemplate Lib() {\n    signal output o;\n    o <== 0;\n}\n",
        )
        .unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ncomponent main = Lib();\n";
        let main_path = ws.join("main.circom");
        fs::write(&main_path, main_src).unwrap();

        let main_url = Url::from_file_path(&main_path).unwrap();
        // Workspace root set so the `include` is loaded (path-traversal confinement).
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state
            .source_db
            .set_document(&main_url, main_src.to_string());
        state.source_db.load_include(&main_url, "lib.circom");

        // The only `Lib` token in main.circom is the reference inside `component main = Lib()`.
        let locs = jump(&state, &main_url, main_src, "Lib", 0);

        assert_eq!(locs.len(), 1, "cross-file main-component jump: {locs:?}");
        assert!(
            locs[0].uri.to_file_path().unwrap().ends_with("lib.circom"),
            "jumps into the included lib: {}",
            locs[0].uri
        );
        // Lands on `template Lib()` — the template name line in lib.circom.
        assert_eq!(
            locs[0].range.start.line, 1,
            "lands on the lib template: {locs:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Goto-definition on a **named** component member field (`c.out`) jumps to the `out` signal
    /// declaration in the receiver's template — the type-inference path the flat resolver omits.
    #[test]
    fn member_field_named_jump_test() {
        let source = "pragma circom 2.0.0;\ntemplate Multiplier2() {\n    signal input in[2];\n    signal output out;\n    out <== in[0] * in[1];\n}\ntemplate Main() {\n    component c = Multiplier2();\n    signal output res;\n    res <== c.out;\n}\n";
        let url = Url::from_file_path("/tmp/mf_named.circom").unwrap();
        let state = state_with(&url, source);

        // `out` occurrences: [0]=decl, [1]=usage in Multiplier2, [2]=the `c.out` field.
        let locs = jump(&state, &url, source, "out", 2);

        assert_eq!(locs.len(), 1, "named member-field jump: {locs:?}");
        assert_eq!(locs[0].uri, url, "jumps within the same file");
        // The `signal output out;` declaration is on line 4 (0-indexed 3).
        assert_eq!(
            locs[0].range.start.line, 3,
            "lands on the out signal: {locs:?}"
        );
    }

    /// Goto-definition on an **anonymous** component member field (`Multiplier2()([a, b]).out`)
    /// jumps to the `out` signal — the reported case. The receiver is a `Call` (no named component
    /// variable), so the template name comes from the callee.
    #[test]
    fn member_field_anonymous_jump_test() {
        let source = "pragma circom 2.0.0;\ntemplate Multiplier2() {\n    signal input in[2];\n    signal output out;\n    out <== in[0] * in[1];\n}\ntemplate Main() {\n    signal input a;\n    signal input b;\n    signal output c;\n    c <== Multiplier2()([a, b]).out;\n}\n";
        let url = Url::from_file_path("/tmp/mf_anon.circom").unwrap();
        let state = state_with(&url, source);

        // `out` occurrences: [0]=decl, [1]=usage in Multiplier2, [2]=the anonymous `.out` field.
        let locs = jump(&state, &url, source, "out", 2);

        assert_eq!(locs.len(), 1, "anonymous member-field jump: {locs:?}");
        assert_eq!(locs[0].uri, url, "jumps within the same file");
        assert_eq!(
            locs[0].range.start.line, 3,
            "lands on the out signal: {locs:?}"
        );
    }

    /// Goto-definition on a **named** member field where the template lives in an `include`d file
    /// jumps across the include to the signal declaration.
    #[test]
    fn member_field_cross_file_jump_test() {
        use std::fs;

        let base = std::env::temp_dir().join(format!("ccls_mf_xfile_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(
            ws.join("lib.circom"),
            "pragma circom 2.0.0;\ntemplate Lib() {\n    signal output o;\n}\n",
        )
        .unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Main() {\n    component c = Lib();\n    signal output res;\n    res <== c.o;\n}\n";
        let main_path = ws.join("main.circom");
        fs::write(&main_path, main_src).unwrap();

        let main_url = Url::from_file_path(&main_path).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state
            .source_db
            .set_document(&main_url, main_src.to_string());
        state.source_db.load_include(&main_url, "lib.circom");

        // `o` occurrences in main: [0]=the `c.o` field (lib's `o` is in another file).
        let locs = jump(&state, &main_url, main_src, "o", 0);

        assert_eq!(locs.len(), 1, "cross-file member-field jump: {locs:?}");
        assert!(
            locs[0].uri.to_file_path().unwrap().ends_with("lib.circom"),
            "jumps into the included lib: {}",
            locs[0].uri
        );
        // lib's `signal output o;` is on line 3 (0-indexed 2).
        assert_eq!(
            locs[0].range.start.line, 2,
            "lands on the lib signal: {locs:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Goto-definition on an **anonymous** member field whose template lives in an `include`d file
    /// (`Lib()().o`) jumps across the include to the signal. Combines the anonymous callee
    /// extraction with cross-file template resolution — both exercised only separately above.
    #[test]
    fn member_field_anonymous_cross_file_jump_test() {
        use std::fs;

        let base = std::env::temp_dir().join(format!("ccls_mfax_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(
            ws.join("lib.circom"),
            "pragma circom 2.0.0;\ntemplate Lib() {\n    signal output o;\n}\n",
        )
        .unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Main() {\n    signal output res;\n    res <== Lib()().o;\n}\n";
        let main_path = ws.join("main.circom");
        fs::write(&main_path, main_src).unwrap();

        let main_url = Url::from_file_path(&main_path).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state
            .source_db
            .set_document(&main_url, main_src.to_string());
        state.source_db.load_include(&main_url, "lib.circom");

        // `o` occurrences in main: [0]=the `Lib()().o` field.
        let locs = jump(&state, &main_url, main_src, "o", 0);

        assert_eq!(
            locs.len(),
            1,
            "cross-file anonymous member-field jump: {locs:?}"
        );
        assert!(
            locs[0].uri.to_file_path().unwrap().ends_with("lib.circom"),
            "jumps into the included lib: {}",
            locs[0].uri
        );
        assert_eq!(
            locs[0].range.start.line, 2,
            "lands on the lib signal: {locs:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `include_target_location` on a **non-sibling** include (`include "lib.circom"` where lib lives under
    /// `circuits/`) resolves to the indexed lib's URL via the basename fallback after
    /// `index_workspace`. The same-dir lookup misses (no sibling), so the project index must drive
    /// the jump — and the jump target must agree with `load_include`'s resolution.
    #[test]
    fn include_target_location_nonsibling_via_index_test() {
        use std::fs;

        let base = std::env::temp_dir().join(format!("ccls_jump_ns_{}", std::process::id()));
        let ws = base.join("ws");
        let circuits = ws.join("circuits");
        fs::create_dir_all(&circuits).unwrap();
        fs::write(circuits.join("lib.circom"), "pragma circom 2.0.0;\n").unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\n";
        let main_path = ws.join("main.circom");
        fs::write(&main_path, main_src).unwrap();

        let main_url = Url::from_file_path(&main_path).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state
            .source_db
            .set_document(&main_url, main_src.to_string());

        let id = state.source_db.id_for_url(&main_url).unwrap();
        let file_db = state.source_db.file_db(id);
        let ast = AstCircomProgram::cast(syntax_tree(main_src)).expect("parses to a program");
        let token = ast
            .syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == TokenKind::CircomString)
            .expect("include path string present");

        let locs = super::include_target_location(&file_db, &token, state.source_db.vfs());
        assert_eq!(
            locs.len(),
            1,
            "non-sibling include jumps to exactly one target"
        );
        let resolved = locs[0].uri.to_file_path().unwrap();
        assert!(
            resolved.ends_with("circuits/lib.circom"),
            "jumps to the indexed non-sibling lib: {resolved:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Repro: open A, jump to its include B, then goto-def a component usage *inside* B. B defines
    /// the template it uses (in-file resolve). Drives the real `cursor_context` gate. The component
    /// usage lives inside a template body (valid circom — top-level instantiation is `main` only).
    #[test]
    fn goto_def_inside_included_file_infile_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_repro_infile_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let b_src = "pragma circom 2.0.0;\ntemplate Comp() {\n    signal output o;\n    o <== 0;\n}\ntemplate Foo() {\n    component c = Comp();\n}\n";
        fs::write(ws.join("B.circom"), b_src).unwrap();
        let a_src = "pragma circom 2.0.0;\ninclude \"B.circom\";\n";
        let a_path = ws.join("A.circom");
        fs::write(&a_path, a_src).unwrap();
        let a_url = Url::from_file_path(&a_path).unwrap();
        let b_url = Url::from_file_path(ws.join("B.circom")).unwrap();

        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state.source_db.set_document(&a_url, a_src.to_string());
        state.source_db.load_include(&a_url, "B.circom");
        state.source_db.set_document(&b_url, b_src.to_string());

        // `Comp` occurrences in B: [0]=decl, [1]=the `component c = Comp()` usage inside `Foo`.
        let locs = goto_at(&state, &b_url, b_src, "Comp", 1);
        assert_eq!(
            locs.len(),
            1,
            "in-file goto-def inside the included B: {locs:?}"
        );
        assert_eq!(locs[0].uri, b_url, "resolves within B");

        let _ = fs::remove_dir_all(&base);
    }

    /// Repro (transitive): A includes B; B includes C; C defines `Comp`; a template in B uses it.
    /// After opening A then B, goto-def `Comp` inside B must resolve across B's include to C.
    #[test]
    fn goto_def_inside_included_file_transitive_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_repro_xfile_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let c_src =
            "pragma circom 2.0.0;\ntemplate Comp() {\n    signal output o;\n    o <== 0;\n}\n";
        fs::write(ws.join("C.circom"), c_src).unwrap();
        let b_src = "pragma circom 2.0.0;\ninclude \"C.circom\";\ntemplate Foo() {\n    component c = Comp();\n}\n";
        fs::write(ws.join("B.circom"), b_src).unwrap();
        let a_src = "pragma circom 2.0.0;\ninclude \"B.circom\";\n";
        let a_path = ws.join("A.circom");
        fs::write(&a_path, a_src).unwrap();
        let a_url = Url::from_file_path(&a_path).unwrap();
        let b_url = Url::from_file_path(ws.join("B.circom")).unwrap();
        let c_url = Url::from_file_path(ws.join("C.circom")).unwrap();

        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        state.source_db.set_document(&a_url, a_src.to_string());
        state.source_db.load_include(&a_url, "B.circom");
        state.source_db.set_document(&b_url, b_src.to_string());
        state.source_db.load_include(&b_url, "C.circom");

        // Only one `Comp` in B — the usage inside `Foo` (occurrence 0).
        let locs = goto_at(&state, &b_url, b_src, "Comp", 0);
        assert_eq!(locs.len(), 1, "transitive goto-def inside B: {locs:?}");
        assert_eq!(locs[0].uri, c_url, "resolves across B's include to C");

        let _ = fs::remove_dir_all(&base);
    }

    /// Repro (transitive, B viewed but NOT didOpen'd): A includes B; B includes C; C defines `Comp`;
    /// a template in B uses it. Open A only (loads B as its include, but NOT B's include C). Goto-def
    /// `Comp` inside B should still resolve — B's includes must load on demand.
    #[test]
    fn goto_def_inside_included_file_transitive_lazy_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_repro_lazy_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let c_src =
            "pragma circom 2.0.0;\ntemplate Comp() {\n    signal output o;\n    o <== 0;\n}\n";
        fs::write(ws.join("C.circom"), c_src).unwrap();
        let b_src = "pragma circom 2.0.0;\ninclude \"C.circom\";\ntemplate Foo() {\n    component c = Comp();\n}\n";
        fs::write(ws.join("B.circom"), b_src).unwrap();
        let a_src = "pragma circom 2.0.0;\ninclude \"B.circom\";\n";
        let a_path = ws.join("A.circom");
        fs::write(&a_path, a_src).unwrap();
        let a_url = Url::from_file_path(&a_path).unwrap();
        let b_url = Url::from_file_path(ws.join("B.circom")).unwrap();
        let c_url = Url::from_file_path(ws.join("C.circom")).unwrap();

        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();
        // Open A only — B is loaded (as A's include) but its include C is NOT loaded.
        state.source_db.set_document(&a_url, a_src.to_string());
        state.source_db.load_include(&a_url, "B.circom");

        let locs = goto_at(&state, &b_url, b_src, "Comp", 0);
        assert_eq!(locs.len(), 1, "lazy transitive goto-def inside B: {locs:?}");
        assert_eq!(
            locs[0].uri, c_url,
            "B's includes load on demand → resolves to C"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
