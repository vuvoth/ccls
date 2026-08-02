//! Salsa-shaped source database over an in-memory [`vfs::Vfs`].
//!
//! Each [`SourceDatabase`] method maps 1:1 to a future salsa query, so adopting salsa is an
//! implementation swap. Split (rust-analyzer style): [`Vfs`] owns file existence + text + change
//! log; [`ContentCacheDb`] owns derived caches (parse/file DB/symbol table), invalidated by
//! *draining the VFS change log*. Guarantees: includes read + parsed **once** then cached; a client
//! resending unchanged text records no change (no reparse).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use lsp_types::Url;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::node::SyntaxNode;
use syntax::tree::syntax_tree;
use vfs::{ChangedFile, Vfs, VfsPath};

use crate::file_db::{FileDB, FileId};
use crate::symbol_table::SymbolTable;

/// Source-level queries over open files, keyed by [`FileId`] — inputs (`file_text`) or
/// content-derived values (`parse`/`ast`/`file_db`/`symbol_table`). All `&self` with memoization
/// via interior mutability; callers get owned values (no borrow across a query).
pub trait SourceDatabase {
    /// The full text of `id` — the single `#[salsa::input]`; everything else is derived.
    fn file_text(&self, id: FileId) -> Arc<str>;
    /// The syntax tree for `id`'s text (memoized).
    fn parse(&self, id: FileId) -> SyntaxNode;
    /// The top-level AST node, or `None` if the parse produced no `CircomProgram` root.
    fn ast(&self, id: FileId) -> Option<AstCircomProgram>;
    /// Offset/line bookkeeping for `id` (memoized).
    fn file_db(&self, id: FileId) -> FileDB;
    /// The lexical symbol table for `id` (memoized); built lazily on first query, dropped by the
    /// change-log invalidation on any edit.
    fn symbol_table(&self, id: FileId) -> Arc<SymbolTable>;
}

/// Lazily-computed caches behind one `RefCell` so a query takes only one short-lived borrow (read
/// for hit-check, write for miss-fill) — never nested. Invalidation is change-log-driven: a
/// mutation drains [`Vfs::take_changes`] and drops only changed ids, so editing file A never
/// recomputes file B. No per-entry content hash — the change log is the only signal.
struct Caches {
    parse: HashMap<FileId, SyntaxNode>,
    file_db: HashMap<FileId, FileDB>,
    symbol_table: HashMap<FileId, Arc<SymbolTable>>,
    /// Cache-miss parses per file (test-only memoization proof). Not touched by invalidation — it
    /// counts total parses, not cache state.
    parse_count: HashMap<FileId, usize>,
}

/// [`SourceDatabase`] backed by a [`Vfs`] + derived caches. Single-threaded server ⇒ `RefCell`
/// suffices for fills behind `&self`.
pub struct ContentCacheDb {
    vfs: Vfs,
    caches: RefCell<Caches>,
}

impl Default for ContentCacheDb {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentCacheDb {
    pub fn new() -> Self {
        Self {
            vfs: Vfs::new(),
            caches: RefCell::new(Caches {
                parse: HashMap::new(),
                file_db: HashMap::new(),
                symbol_table: HashMap::new(),
                parse_count: HashMap::new(),
            }),
        }
    }

    /// Set the workspace roots confining `include` resolution (delegated to the [`Vfs`], which owns
    /// them + the pure containment check).
    pub fn set_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        self.vfs.set_workspace_roots(roots);
    }

    /// Read-only [`Vfs`] handle (e.g. so `jump_to_lib` applies the same containment check as
    /// `load_include`).
    pub(crate) fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    /// Convert a `file:` URL to its absolutized [`VfsPath`], or `None` for non-`file:` schemes or
    /// non-absolutizable paths. Single source for the URI→path step so interning and lookup can't
    /// disagree (a split would intern under one key and look up another, silently breaking
    /// resolution).
    fn url_to_vpath(url: &Url) -> Option<VfsPath> {
        let path = url.to_file_path().ok()?;
        VfsPath::from_abs_path(&path)
    }

    /// Resolve a relative include `rel` against `parent_url`'s dir to an absolutized [`VfsPath`].
    /// Shared by [`Self::load_include`], [`Self::id_for_include`], and goto-def so all three agree
    /// on the resolved path.
    fn resolve_include(parent_url: &Url, rel: &str) -> Option<VfsPath> {
        let parent_path = parent_url.to_file_path().ok()?;
        let parent_dir = parent_path.parent()?;
        let lib_path = parent_dir.join(rel);
        VfsPath::from_abs_path(&lib_path)
    }

    /// The `FileId` for `url`, from its absolutized path (so aliased paths collapse to one id).
    /// `None` for non-`file:` schemes or non-absolutizable paths — callers skip indexing those.
    pub fn id_for_url(&self, url: &Url) -> Option<FileId> {
        self.vfs.file_id(&Self::url_to_vpath(url)?)
    }

    /// The already-interned `FileId` for an include, without reading disk. `None` if unresolvable
    /// or not yet loaded. Used by cross-file goto-def (the include loads once in `handle_update`).
    pub fn id_for_include(&self, parent_url: &Url, rel: &str) -> Option<FileId> {
        let vpath = Self::resolve_include(parent_url, rel)?;
        self.vfs.file_id(&vpath)
    }

    /// Register/update a document's text. Returns `(FileId, changed)`; `changed` is `false` when
    /// text was identical (no-op — no change recorded, no cache drop). `None` for non-`file:` URLs.
    pub fn set_document(&mut self, url: &Url, text: String) -> Option<(FileId, bool)> {
        let vpath = Self::url_to_vpath(url)?;
        let id = self.vfs.set_file_contents(vpath, Some(Arc::from(text)));
        let changes = self.invalidate_changed();
        let changed = changes.iter().any(|c| c.file_id == id);
        Some((id, changed))
    }

    /// Load a relative include from disk **once**, then serve the interned `FileId` from cache — a
    /// keystroke in the main file never re-reads its includes. `None` (skipped) for non-`file:`
    /// schemes, missing parent dir, unreadable file, or non-absolutizable path.
    pub fn load_include(&mut self, parent_url: &Url, rel: &str) -> Option<FileId> {
        let vpath = Self::resolve_include(parent_url, rel)?;

        // Security: confine the resolved include to a workspace root. `canonicalize` (the one disk
        // stat — kept in this LSP layer so Vfs stays I/O-free) resolves `..`/`.`/symlinks to a real
        // absolute path; `is_confined` then checks it's inside a root. Escapes (`include
        // "/etc/passwd"`, `../../.ssh/id_rsa`, or a root symlink pointing out) are refused. This is
        // the single read boundary; lookup/jump paths only surface includes loaded (and thus
        // confined) here.
        let canonical = vpath.as_path().canonicalize().ok()?;
        if !self.vfs.is_confined(&canonical) {
            return None;
        }

        // Already loaded — serve the cached id (never re-read).
        if let Some(id) = self.vfs.file_id(&vpath) {
            return Some(id);
        }

        let src = std::fs::read_to_string(vpath.as_path()).ok()?;
        let id = self.vfs.set_file_contents(vpath, Some(Arc::from(src)));
        self.invalidate_changed();
        Some(id)
    }

    /// Drain the VFS change log and drop every changed id's derived caches (others untouched).
    /// `parse_count` is intentionally preserved — it tracks total parses, not cache state.
    fn invalidate_changed(&mut self) -> Vec<ChangedFile> {
        let changes = self.vfs.take_changes();
        if !changes.is_empty() {
            let mut caches = self.caches.borrow_mut();
            for ChangedFile { file_id, .. } in &changes {
                caches.parse.remove(file_id);
                caches.file_db.remove(file_id);
                caches.symbol_table.remove(file_id);
            }
        }
        changes
    }

    /// Reconstruct the `file:` URL for `id` from its VFS path. Safe — `id` was interned from a
    /// `file:`-scheme URL at registration.
    fn url_for(&self, id: FileId) -> Url {
        let vpath = self
            .vfs
            .path(id)
            .expect("url_for: unknown FileId (document not registered)");
        Url::from_file_path(vpath.as_path())
            .expect("VfsPath was validated as file: scheme at registration time")
    }

    /// Test-only: how many cache-miss parses have run for `id` (0 = never parsed).
    #[cfg(test)]
    pub fn parse_count(&self, id: FileId) -> usize {
        self.caches
            .borrow()
            .parse_count
            .get(&id)
            .copied()
            .unwrap_or(0)
    }
}

impl SourceDatabase for ContentCacheDb {
    fn file_text(&self, id: FileId) -> Arc<str> {
        self.vfs
            .file_text(id)
            .expect("file_text: unknown or deleted FileId (document not registered)")
    }

    fn parse(&self, id: FileId) -> SyntaxNode {
        // Hit check under a short-lived shared borrow (dropped at block end).
        {
            let caches = self.caches.borrow();
            if let Some(cached) = caches.parse.get(&id) {
                return cached.clone();
            }
        }

        let text = self.file_text(id);
        let tree = syntax_tree(&text);
        let mut caches = self.caches.borrow_mut();
        caches.parse.insert(id, tree.clone());
        *caches.parse_count.entry(id).or_insert(0) += 1;
        tree
    }

    fn ast(&self, id: FileId) -> Option<AstCircomProgram> {
        AstCircomProgram::cast(self.parse(id))
    }

    fn file_db(&self, id: FileId) -> FileDB {
        {
            let caches = self.caches.borrow();
            if let Some(cached) = caches.file_db.get(&id) {
                return cached.clone();
            }
        }

        let text = self.file_text(id);
        let url = self.url_for(id);
        // `id` and `url` were validated as `file:`-scheme at registration, so `new` (skips
        // re-hashing) is safe.
        let file_db = FileDB::new(id, &text, url);
        self.caches.borrow_mut().file_db.insert(id, file_db.clone());
        file_db
    }

    fn symbol_table(&self, id: FileId) -> Arc<SymbolTable> {
        {
            let caches = self.caches.borrow();
            if let Some(cached) = caches.symbol_table.get(&id) {
                return cached.clone();
            }
        }

        // Build the per-file `SymbolTable` from the cached parse + file DB; a non-program parse
        // yields an empty table (no `unwrap`).
        let table = match self.ast(id) {
            Some(ast) => {
                let file_db = self.file_db(id);
                Arc::new(SymbolTable::build(&file_db, &ast))
            }
            None => Arc::new(SymbolTable::default()),
        };
        self.caches
            .borrow_mut()
            .symbol_table
            .insert(id, table.clone());
        table
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::Url;

    use super::*;

    fn url_for(name: &str) -> Url {
        Url::from_file_path(format!("/tmp/ccls_test/{name}.circom")).unwrap()
    }

    #[test]
    fn non_file_scheme_is_skipped() {
        let mut db = ContentCacheDb::new();
        let untitled = Url::parse("untitled:Untitled-1").unwrap();
        // set_document returns None for non-`file:` URIs, so untitled/git docs are never indexed.
        assert!(db.set_document(&untitled, "x".to_string()).is_none());
        assert!(db.id_for_url(&untitled).is_none());
    }

    #[test]
    fn parse_memoized_by_content_test() {
        let mut db = ContentCacheDb::new();
        let url = url_for("memo");
        let (id, changed) = db
            .set_document(&url, "pragma circom 2.0.0;".to_string())
            .unwrap();
        assert!(changed, "first registration is a real change");

        db.parse(id);
        assert_eq!(db.parse_count(id), 1, "first query parses");

        // Repeated query with no text change → cache hit, no recompute.
        db.parse(id);
        assert_eq!(db.parse_count(id), 1, "second query reuses the cached tree");

        // Different text → invalidation → recompute.
        let (_, changed2) = db
            .set_document(&url, "pragma circom 2.1.0;".to_string())
            .unwrap();
        assert!(changed2, "different text is a real change");
        db.parse(id);
        assert_eq!(db.parse_count(id), 2, "content change forces a reparse");

        // Re-submitting identical text is a no-op: no change reported, no reparse.
        let (_, changed3) = db
            .set_document(&url, "pragma circom 2.1.0;".to_string())
            .unwrap();
        assert!(!changed3, "identical text is a no-op");
        db.parse(id);
        assert_eq!(db.parse_count(id), 2, "no-op resend does not reparse");

        // Same id, different file: editing file B never touches file A's cache.
        let (other, _) = db
            .set_document(&url_for("other"), "template T() {}".to_string())
            .unwrap();
        db.parse(other);
        assert_eq!(
            db.parse_count(id),
            2,
            "parsing another file leaves this one cached"
        );
    }

    #[test]
    fn file_db_memoized_and_consistent_with_text_test() {
        let mut db = ContentCacheDb::new();
        let url = url_for("fdb");
        let (id, _) = db.set_document(&url, "a\nb\nc".to_string()).unwrap();
        let first = db.file_db(id);
        // newline_offsets records byte offsets of '\n' -> [1, 3].
        assert_eq!(first.newline_offsets, vec![1, 3]);
        // Re-query returns the cached FileDB (clone of the same value).
        let again = db.file_db(id);
        assert_eq!(first.newline_offsets, again.newline_offsets);
    }

    /// The symbol table builds lazily from the cached parse, is memoized (same `Arc` on repeat),
    /// is non-empty for a real program, and is dropped on a content change.
    #[test]
    fn symbol_table_memoized_and_non_empty_test() {
        let mut db = ContentCacheDb::new();
        let url = url_for("sym");
        let src = "pragma circom 2.0.0;
template Multiplier2() {
    signal input a;
    signal input b;
    signal output c;
    c <== a * b;
}
"
        .to_string();
        let (id, _) = db.set_document(&url, src).unwrap();

        let first = db.symbol_table(id);
        // Non-empty: top-level carries `Multiplier2`; member counts are the real correctness
        // signal.
        assert!(
            !first.lookup_top_level("Multiplier2").is_empty(),
            "template name should be indexed in the file top-level"
        );

        // Memoized: a repeat query returns the same `Arc` (no rebuild).
        let again = db.symbol_table(id);
        assert!(
            Arc::ptr_eq(&first, &again),
            "repeat symbol_table query must hit the cache"
        );

        // A content change drops the cached table (a new `Arc` is built).
        let (_, changed) = db
            .set_document(&url, "pragma circom 2.1.0;\n".to_string())
            .unwrap();
        assert!(changed, "different text is a real change");
        let rebuilt = db.symbol_table(id);
        assert!(
            !Arc::ptr_eq(&first, &rebuilt),
            "content change must invalidate the cached symbol table"
        );
        // The new (now template-less) program has an empty file top-level.
        assert!(
            rebuilt.lookup_top_level("Multiplier2").is_empty(),
            "the edited program has no Multiplier2 template"
        );
    }

    /// Path-traversal confinement: an `include` that resolves outside the workspace root is refused
    /// (never read), whether via `..`, an absolute path, or a symlink. Set up a workspace dir, a
    /// main file inside it, and a secret file one level above; both escape forms must return `None`.
    #[test]
    fn include_confined_to_workspace_root_test() {
        use std::fs;
        use std::path::PathBuf;

        let base = std::env::temp_dir().join(format!("ccls_confine_{}", std::process::id()));
        let ws = base.join("ws");
        let secret = base.join("secret.circom");
        let main_path = ws.join("main.circom");
        fs::create_dir_all(&ws).unwrap();
        fs::write(&secret, "pragma circom 2.0.0;").unwrap();
        fs::write(&main_path, "pragma circom 2.0.0;").unwrap();

        let main_url = Url::from_file_path(&main_path).unwrap();
        let mut db = ContentCacheDb::new();
        // Root is the workspace dir only — `secret.circom` lives above it.
        db.set_workspace_roots(vec![ws.canonicalize().unwrap()]);

        // No roots configured yet in the empty case: every include is refused (fail closed).
        let mut empty = ContentCacheDb::new();
        assert!(
            empty.load_include(&main_url, "lib.circom").is_none(),
            "with no workspace roots, no include may be loaded (fail closed)"
        );

        // `..` escape to a sibling file outside the root is refused.
        assert!(
            db.load_include(&main_url, "../secret.circom").is_none(),
            "include escaping the workspace via `..` must be refused"
        );
        // Absolute path outside the root is refused (`PathBuf::join` would otherwise replace base).
        assert!(
            db.load_include(&main_url, secret.to_str().unwrap())
                .is_none(),
            "an absolute include outside the workspace must be refused"
        );

        // Sanity: a same-directory include inside the root loads (file must exist).
        let in_root = ws.join("lib.circom");
        fs::write(&in_root, "pragma circom 2.0.0;").unwrap();
        assert!(
            db.load_include(&main_url, "lib.circom").is_some(),
            "an include inside the workspace root must load"
        );

        let _ = PathBuf::from(&base);
        let _ = fs::remove_dir_all(&base);
    }
}
