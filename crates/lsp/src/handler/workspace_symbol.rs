//! Workspace-wide symbol search (`workspace/symbol`): every template/function/bus across the
//! workspace whose name matches the query. Cheap now that the workspace is eagerly indexed — each
//! file's `SymbolTable` is pre-warmed.

use anyhow::Result;
use lsp_types::{
    SymbolInformation, SymbolKind as LspSymbolKind, WorkspaceSymbolParams, WorkspaceSymbolResponse,
};

use crate::global_state::GlobalState;
use crate::source_db::SourceDatabase;
use crate::symbol_table::SymbolKind;

/// Entry point for `workspace/symbol`. Returns matching top-level symbols as flat
/// `SymbolInformation`s (location = the symbol's name token in its file), or `None` for an empty
/// result. Never errors.
#[allow(deprecated)] // `SymbolInformation::deprecated` is required by lsp-types 0.94 (use `tags`).
pub fn handle(
    state: &GlobalState,
    params: WorkspaceSymbolParams,
) -> Result<Option<WorkspaceSymbolResponse>> {
    let symbols = state.workspace_symbols(&params.query);
    let infos: Vec<SymbolInformation> = symbols
        .into_iter()
        .map(|(id, sym)| SymbolInformation {
            name: sym.name,
            kind: to_lsp_kind(sym.kind),
            tags: None,
            deprecated: None,
            location: lsp_types::Location {
                uri: state.source_db.file_db(id).file_path.clone(),
                range: sym.def_range,
            },
            container_name: None,
        })
        .collect();
    Ok(if infos.is_empty() {
        None
    } else {
        Some(WorkspaceSymbolResponse::Flat(infos))
    })
}

/// Map a circom [`SymbolKind`] to the closest LSP [`LspSymbolKind`].
fn to_lsp_kind(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Template => LspSymbolKind::CLASS,
        SymbolKind::Function => LspSymbolKind::FUNCTION,
        SymbolKind::Bus => LspSymbolKind::STRUCT,
        _ => LspSymbolKind::VARIABLE,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use lsp_types::{WorkspaceSymbolParams, WorkspaceSymbolResponse};

    use crate::global_state::GlobalState;

    use super::handle;

    fn names(state: &GlobalState, query: &str) -> Vec<String> {
        let resp = handle(
            state,
            WorkspaceSymbolParams {
                query: query.to_string(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
        .unwrap();
        match resp {
            Some(WorkspaceSymbolResponse::Flat(infos)) => {
                infos.into_iter().map(|i| i.name).collect()
            }
            _ => Vec::new(),
        }
    }

    /// `workspace/symbol` lists every top-level template/function across the workspace when the
    /// query is blank, and filters case-insensitively otherwise. Bodies (signals/vars) are excluded.
    #[test]
    fn workspace_symbol_lists_and_filters_test() {
        let base = std::env::temp_dir().join(format!("ccls_ws_sym_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_src = "pragma circom 2.0.0;\ntemplate Lib() {\n    signal output o;\n    o <== 0;\n}\nfunction helper(x) {\n    return x;\n}\n";
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ntemplate Main() {\n    signal output out;\n    out <== 0;\n}\n";
        fs::write(ws.join("lib.circom"), lib_src).unwrap();
        fs::write(ws.join("main.circom"), main_src).unwrap();

        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();

        let all = names(&state, "");
        assert!(all.contains(&"Lib".to_string()), "Lib listed: {all:?}");
        assert!(all.contains(&"Main".to_string()), "Main listed: {all:?}");
        assert!(
            all.contains(&"helper".to_string()),
            "function helper listed: {all:?}"
        );
        // Body-only symbols (signals) are not top-level, so never listed.
        assert!(
            !all.iter().any(|n| n == "o" || n == "out"),
            "signals are not workspace symbols: {all:?}"
        );

        // Case-insensitive substring filter.
        let only_lib = names(&state, "lib");
        assert_eq!(only_lib, vec!["Lib".to_string()], "filter to Lib");

        let only_main = names(&state, "MA");
        assert_eq!(
            only_main,
            vec!["Main".to_string()],
            "case-insensitive match"
        );

        // No match ⇒ empty result (`None` from the handler).
        assert!(names(&state, "zzz").is_empty(), "no match yields empty");

        let _ = fs::remove_dir_all(&base);
    }
}
