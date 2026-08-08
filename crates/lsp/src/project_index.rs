//! Project-wide `.circom` file discovery + indexing (the only place in the server that walks the
//! filesystem).
//!
//! The pure basename index lives in [`vfs`]; this module is the I/O boundary that feeds it. At
//! `initialize` (and on workspace-folder changes) [`collect_circom_files`] walks each root once and
//! [`GlobalState::index_workspace`] interns every found path with **no text** (cheap — no file
//! reads). Text loads lazily, on demand, the first time an `include` resolves to that path.

use std::path::PathBuf;

use vfs::VfsPath;

use crate::global_state::GlobalState;

/// Recursively collect every `*.circom` file under `roots`, returning each as a **canonical**
/// absolute path. Descends into `node_modules` (circomlib lives there), so this deliberately does
/// NOT use an ignore-respecting walker. Symlinks are not followed (`walkdir` default), which breaks
/// symlink loops; entries that fail to canonicalize (raced-away, permission-denied) are skipped.
pub(crate) fn collect_circom_files(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            // Use the type walkdir already determined (often via `d_type`, no extra syscall) instead
            // of `path.is_file()` which issues a fresh `stat` per entry. `file_type()` also skips
            // symlinks-to-files, consistent with `follow_links(false)` above.
            if !entry.file_type().is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("circom") {
                continue;
            }
            // Canonicalize BEFORE interning: `VfsPath::from_abs_path` only *absolutizes* (lexical,
            // no symlink/`..` resolution), so confinement's `starts_with` against canonical roots
            // needs a real canonical path here. Skip entries that vanish mid-walk.
            if let Ok(canon) = path.canonicalize() {
                out.push(canon);
            }
        }
    }
    out
}

impl GlobalState {
    /// Intern a pre-collected list of **canonical** `.circom` paths (path-only, no text) into the
    /// basename index. The I/O-heavy collection ([`collect_circom_files`]) can run on a background
    /// thread and hand its result here; this step is pure interning + cache flush. Idempotent —
    /// re-registering existing paths never clobbers text a `load_include`/`didOpen` loaded.
    pub(crate) fn register_indexed_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        for canon in paths {
            if let Some(vpath) = VfsPath::from_abs_path(&canon) {
                self.source_db.vfs_mut().register_path(vpath);
            }
        }
        // register_path records no change-log entry, so this flush is usually a no-op; new files in
        // the index can newly satisfy an include, so the resolved-include cache is dropped too.
        self.source_db.invalidate_changed();
        self.drop_include_cache();
    }

    /// Eagerly intern every `.circom` file under the **current** workspace roots (path-only, no
    /// text). Used by tests and as the synchronous fallback; the live `main_loop` backgrounds the
    /// walk and calls [`Self::register_indexed_paths`] instead so init never blocks on I/O.
    pub fn index_workspace(&mut self) {
        let roots: Vec<PathBuf> = self.source_db.vfs().workspace_roots().to_vec();
        self.register_indexed_paths(collect_circom_files(&roots));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
