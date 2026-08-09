//! Project-wide `.circom` file discovery + indexing — the only place that walks the filesystem.
//! `collect_circom_files` runs (backgrounded) at `initialize` and on workspace-folder changes; it
//! interns paths with no text, and text loads lazily on first `include` resolution.

use std::path::PathBuf;
use std::sync::Arc;

use vfs::VfsPath;

use crate::global_state::GlobalState;
use crate::source_db::SourceDatabase;

/// `.git`/`target` never hold circom sources; `node_modules` is kept (circomlib lives there).
fn is_pruned_dir(file_name: &std::ffi::OsStr) -> bool {
    matches!(file_name.to_str(), Some(".git") | Some("target"))
}

/// Recursively collect every `*.circom` file under `roots` as canonical absolute paths. Descends
/// into `node_modules`, prunes `.git`/`target`, and doesn't follow symlinks.
pub(crate) fn collect_circom_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_pruned_dir(e.file_name()))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            // `file_type()` reuses walkdir's cached type (no extra `stat`) and skips symlinks.
            if !entry.file_type().is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("circom") {
                continue;
            }
            // Canonicalize here: `VfsPath::from_abs_path` only absolutizes, but confinement needs a
            // real canonical path. Skip entries that vanish mid-walk.
            if let Ok(canon) = path.canonicalize() {
                out.push(canon);
            }
        }
    }
    out
}

/// Walk `roots` and return each `*.circom` file as a canonical path **plus** its content. Pure I/O —
/// safe to run on the background thread (text interning stays on the main thread). Files that
/// vanish between the walk and the read are skipped.
pub(crate) fn collect_circom_files_with_content(roots: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for canon in collect_circom_files(roots) {
        if let Ok(meta) = std::fs::metadata(&canon) {
            if meta.len() > crate::source_db::MAX_FILE_BYTES {
                eprintln!(
                    "ccls: skipping {} ({} bytes > limit)",
                    canon.display(),
                    meta.len()
                );
                continue;
            }
        }
        if let Ok(content) = std::fs::read_to_string(&canon) {
            out.push((canon, content));
        }
    }
    out
}

impl GlobalState {
    /// Intern workspace `.circom` paths **with their text** (eager load so workspace occurrences/
    /// symbol can scan every file), record their ids, and re-load each open doc's includes — a
    /// non-sibling include that missed at open time (walk hadn't landed) becomes resolvable now.
    /// Idempotent; never clobbers loaded text.
    pub(crate) fn register_indexed_paths(&mut self, entries: Vec<(PathBuf, String)>) {
        if entries.is_empty() {
            return;
        }
        for (canon, content) in entries {
            let Some(vpath) = VfsPath::from_abs_path(&canon) else {
                continue;
            };
            let id = self.source_db.vfs_mut().register_path(vpath.clone());
            self.workspace_files.insert(id);
            self.source_db
                .vfs_mut()
                .set_file_contents(vpath, Some(Arc::from(content)));
        }
        self.source_db.invalidate_changed();
        self.reload_open_doc_includes();
        self.drop_include_cache();
    }

    /// Re-load includes for every open document. Includes are collected first so the AST borrow is
    /// dropped before the mutable `load_include`.
    fn reload_open_doc_includes(&mut self) {
        let open: Vec<lsp_types::Url> = self.open_documents.iter().cloned().collect();
        for uri in open {
            let Some(id) = self.source_db.id_for_url(&uri) else {
                continue;
            };
            let libs: Vec<String> = self
                .source_db
                .ast(id)
                .map(|a| a.include_paths())
                .unwrap_or_default();
            for rel in libs {
                let _ = self.source_db.load_include(&uri, &rel);
            }
        }
    }

    /// Synchronous full re-walk of the current roots (tests / fallback). The live `main_loop`
    /// backgrounds the walk and calls [`Self::register_indexed_paths`] instead.
    pub fn index_workspace(&mut self) {
        let roots: Vec<PathBuf> = self.source_db.vfs().workspace_roots().to_vec();
        self.register_indexed_paths(collect_circom_files_with_content(&roots));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pruned_dirs_are_skipped_test() {
        let base = std::env::temp_dir().join(format!("ccls_prune_{}", std::process::id()));
        let ws = base.join("ws");
        std::fs::create_dir_all(ws.join("target")).unwrap();
        std::fs::create_dir_all(ws.join(".git")).unwrap();
        std::fs::write(ws.join("main.circom"), "pragma circom 2.0.0;").unwrap();
        std::fs::write(ws.join("target/out.circom"), "pragma circom 2.0.0;").unwrap();
        std::fs::write(ws.join(".git/config.circom"), "pragma circom 2.0.0;").unwrap();

        let found = collect_circom_files(std::slice::from_ref(&ws));
        let names: Vec<String> = found
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        assert!(
            names.contains(&"main.circom".to_string()),
            "top-level kept: {names:?}"
        );
        assert!(
            !names
                .iter()
                .any(|n| n.starts_with("out") || n.starts_with("config")),
            "target/.git pruned: {names:?}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn collect_circom_files_walks_and_canonicalizes_test() {
        let base = std::env::temp_dir().join(format!("ccls_walk_{}", std::process::id()));
        let ws = base.join("ws");
        let node_modules = ws.join("node_modules").join("circomlib").join("circuits");
        std::fs::create_dir_all(&node_modules).unwrap();
        std::fs::write(ws.join("main.circom"), "pragma circom 2.0.0;").unwrap();
        std::fs::write(
            node_modules.join("comparators.circom"),
            "pragma circom 2.0.0;",
        )
        .unwrap();
        // Non-circom files are ignored.
        std::fs::write(ws.join("readme.md"), "# nope").unwrap();

        let found = collect_circom_files(std::slice::from_ref(&ws));
        let names: Vec<String> = found
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        assert!(
            names.contains(&"main.circom".to_string()),
            "walks top-level: {names:?}"
        );
        assert!(
            names.contains(&"comparators.circom".to_string()),
            "descends into node_modules: {names:?}"
        );
        assert!(
            !names.contains(&"readme.md".to_string()),
            "ignores non-.circom files: {names:?}"
        );
        // Every result is canonical (absolute, no `..`).
        for p in &found {
            assert_eq!(p, &p.canonicalize().unwrap(), "result is canonical");
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}
