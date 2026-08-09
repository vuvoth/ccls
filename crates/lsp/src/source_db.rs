//! Salsa-shaped source database over an in-memory [`vfs::Vfs`].
//!
//! Each [`SourceDatabase`] method maps 1:1 to a future salsa query, so adopting salsa is an
//! implementation swap. Split (rust-analyzer style): [`Vfs`] owns file existence + text + change
//! log; [`ContentCacheDb`] owns derived caches (parse/file DB/symbol table), invalidated by
//! *draining the VFS change log*. Guarantees: includes read + parsed **once** then cached; a client
//! resending unchanged text records no change (no reparse).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use lsp_types::Url;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::node::SyntaxNode;
use syntax::tree::{parse as parse_tree, SyntaxError};
use vfs::{ChangedFile, Vfs, VfsPath};

use crate::file_db::{FileDB, FileId};
use crate::symbol_table::SymbolTable;

/// Skip `.circom` files larger than this. Circom codegen can emit multi-MB files that would stall
/// the single-threaded server to parse/index.
pub(crate) const MAX_FILE_BYTES: u64 = 1 << 20; // 1 MiB

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
    /// The syntax errors for `id` (memoized); lexer + parser errors aggregated at parse time.
    fn errors(&self, id: FileId) -> Arc<Vec<SyntaxError>>;
}

/// Lazily-computed caches behind one `RefCell` so a query takes only one short-lived borrow (read
/// for hit-check, write for miss-fill) — never nested. Invalidation is change-log-driven: a
/// mutation drains [`Vfs::take_changes`] and drops only changed ids, so editing file A never
/// recomputes file B. No per-entry content hash — the change log is the only signal.
struct Caches {
    parse: HashMap<FileId, SyntaxNode>,
    file_db: HashMap<FileId, FileDB>,
    symbol_table: HashMap<FileId, Arc<SymbolTable>>,
    errors: HashMap<FileId, Arc<Vec<SyntaxError>>>,
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
                errors: HashMap::new(),
                parse_count: HashMap::new(),
            }),
        }
    }

    /// Set the workspace roots scoping the project `.circom` walk (the basename-index source).
    /// `include` resolution itself is circom-style (relative to the source file), not confined.
    pub fn set_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        self.vfs.set_workspace_roots(roots);
    }

    /// Read-only [`Vfs`] handle.
    pub(crate) fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    /// Mutable [`Vfs`] handle for the workspace walker and file-watcher refresh.
    pub(crate) fn vfs_mut(&mut self) -> &mut Vfs {
        &mut self.vfs
    }

    /// `file:` URL → absolutized [`VfsPath`] (`None` for other schemes). Single source so interning
    /// and lookup agree on the path key.
    fn url_to_vpath(url: &Url) -> Option<VfsPath> {
        let path = url.to_file_path().ok()?;
        VfsPath::from_abs_path(&path)
    }

    /// Resolve a relative include `rel` against `parent_url`'s dir to an absolutized [`VfsPath`].
    /// An **absolute** `rel` is refused: `PathBuf::join` would *replace* the base (`include
    /// "/etc/passwd"`), enabling an arbitrary local-file read. circom's legitimate includes are
    /// relative (`..` allowed) and resolve against the project tree; absolute include strings are
    /// not a real circom idiom. (The basename fallback still fields any `rel` but only over the
    /// workspace-indexed set, so it stays safe.)
    fn resolve_include(parent_url: &Url, rel: &str) -> Option<VfsPath> {
        if std::path::Path::new(rel).is_absolute() {
            return None;
        }
        let parent_path = parent_url.to_file_path().ok()?;
        let parent_dir = parent_path.parent()?;
        let lib_path = parent_dir.join(rel);
        VfsPath::from_abs_path(&lib_path)
    }

    /// The `FileId` for `url` (`None` for non-`file:` URIs).
    pub fn id_for_url(&self, url: &Url) -> Option<FileId> {
        self.vfs.file_id(&Self::url_to_vpath(url)?)
    }

    /// The already-interned `FileId` for an include, without reading disk. Same-dir lookup first,
    /// then the project-wide basename fallback ([`Vfs::find_include`]). Pure — it does NOT confine
    /// (no `canonicalize`); stale removed-folder includes are dropped at removal time instead
    /// (`Vfs::unregister_under`).
    pub fn id_for_include(&self, parent_url: &Url, rel: &str) -> Option<FileId> {
        if let Some(vpath) = Self::resolve_include(parent_url, rel) {
            if let Some(id) = self.vfs.file_id(&vpath) {
                return Some(id);
            }
        }
        let parent_vpath = Self::url_to_vpath(parent_url)?;
        self.vfs.find_include(&parent_vpath, rel)
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

    /// Load a relative include from disk once (then serve the cached id), and load its includes
    /// transitively — so goto-def/hover *inside* an include (e.g. one opened via peek/jump without a
    /// full didOpen) can still resolve across that include's own includes. Same-dir path first, then
    /// the project-wide basename fallback. `None` for non-`file:` URIs, missing/unreadable files, or
    /// absolute include paths.
    pub fn load_include(&mut self, parent_url: &Url, rel: &str) -> Option<FileId> {
        let id = self.load_one_include(parent_url, rel)?;
        let mut visited = HashSet::new();
        visited.insert(id);
        self.load_transitive_includes(id, &mut visited);
        Some(id)
    }

    /// The on-disk target of include `rel` from `parent_url`, mirroring [`Self::load_one_include`]'s
    /// path selection: the same-dir relative target if it is a real file, else the workspace-indexed
    /// basename winner (also confirmed to still exist). Refuses absolute `rel`. **Pure** (no read) —
    /// shared by goto-lib (build a URL) and the loader (read + intern), so the jump target and the
    /// loaded file always agree and there is one resolution order to maintain.
    pub(crate) fn resolve_include_vpath(&self, parent_url: &Url, rel: &str) -> Option<VfsPath> {
        if let Some(vpath) = Self::resolve_include(parent_url, rel) {
            if vpath.as_path().is_file() {
                return Some(vpath);
            }
        }
        let parent_vpath = Self::url_to_vpath(parent_url)?;
        let winner = self.vfs.find_include(&parent_vpath, rel)?;
        let winner_path = self.vfs.path(winner)?;
        winner_path.as_path().is_file().then(|| winner_path.clone())
    }

    /// Load a single include (no transitive closure) via [`Self::resolve_include_vpath`].
    fn load_one_include(&mut self, parent_url: &Url, rel: &str) -> Option<FileId> {
        let vpath = self.resolve_include_vpath(parent_url, rel)?;
        self.load_from_disk(&vpath)
    }

    /// Recursively load `id`'s own includes (its include-closure) so resolution works from within
    /// `id`. `visited` breaks include cycles; already-loaded files are served from cache (no re-read).
    fn load_transitive_includes(&mut self, id: FileId, visited: &mut HashSet<FileId>) {
        let Some(path) = self.vfs.path(id) else {
            return;
        };
        let Ok(parent_url) = Url::from_file_path(path.as_path()) else {
            return;
        };
        let includes: Vec<String> = self.ast(id).map(|a| a.include_paths()).unwrap_or_default();
        for rel in includes {
            if let Some(child) = self.load_one_include(&parent_url, &rel) {
                if visited.insert(child) {
                    self.load_transitive_includes(child, visited);
                }
            }
        }
    }

    /// Read one include path from disk — the single disk boundary. A relative include resolves the
    /// way circom resolves it: relative to the **including source file**, so it loads regardless of
    /// the editor's (possibly missing/wrong) workspace root. Both callers produce trusted paths:
    /// the relative path is built from an opened doc's location, and the basename fallback
    /// ([`Vfs::find_include`]) only returns files the workspace walk already indexed. Serves a
    /// cached id when the text is already loaded.
    fn load_from_disk(&mut self, vpath: &VfsPath) -> Option<FileId> {
        if let Some(id) = self.vfs.file_id(vpath) {
            if self.vfs.file_text(id).is_some() {
                return Some(id);
            }
        }
        if let Ok(meta) = std::fs::metadata(vpath.as_path()) {
            if meta.len() > MAX_FILE_BYTES {
                eprintln!(
                    "ccls: skipping {} ({} bytes > {MAX_FILE_BYTES} limit)",
                    vpath.as_path().display(),
                    meta.len()
                );
                return None;
            }
        }
        let src = std::fs::read_to_string(vpath.as_path()).ok()?;
        let id = self
            .vfs
            .set_file_contents(vpath.clone(), Some(Arc::from(src)));
        self.invalidate_changed();
        Some(id)
    }

    /// Drain the VFS change log and drop changed ids' derived caches (`pub(crate)` so the watcher
    /// can flush a `Delete`). `parse_count` is preserved — it tracks total parses, not cache state.
    pub(crate) fn invalidate_changed(&mut self) -> Vec<ChangedFile> {
        let changes = self.vfs.take_changes();
        if !changes.is_empty() {
            let mut caches = self.caches.borrow_mut();
            for ChangedFile { file_id, .. } in &changes {
                caches.parse.remove(file_id);
                caches.file_db.remove(file_id);
                caches.symbol_table.remove(file_id);
                caches.errors.remove(file_id);
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

    /// Lazily parse `id` once and cache both the tree and its errors. Shared by the `parse` and
    /// `errors` queries so a single miss fills both caches.
    fn ensure_parsed(&self, id: FileId) {
        if self.caches.borrow().parse.contains_key(&id) {
            return;
        }
        let text = self.file_text(id);
        let parsed = parse_tree(&text);
        let mut caches = self.caches.borrow_mut();
        caches.parse.insert(id, parsed.tree);
        caches.errors.insert(id, Arc::new(parsed.errors));
        *caches.parse_count.entry(id).or_insert(0) += 1;
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
        self.ensure_parsed(id);
        self.caches
            .borrow()
            .parse
            .get(&id)
            .cloned()
            .expect("ensure_parsed caches the tree")
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

    fn errors(&self, id: FileId) -> Arc<Vec<SyntaxError>> {
        self.ensure_parsed(id);
        self.caches
            .borrow()
            .errors
            .get(&id)
            .cloned()
            .expect("ensure_parsed caches errors")
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::Url;

    use super::*;

    fn url_for(name: &str) -> Url {
        Url::from_file_path(std::env::temp_dir().join(format!("{name}.circom"))).unwrap()
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

    /// Includes resolve relative to the **source file** the way circom resolves them — independent
    /// of the editor's workspace root. So a relative include loads even with no roots configured,
    /// and even when it points outside the (possibly wrong) root. (The basename fallback is still
    /// workspace-index-scoped.) Sanity: a same-directory include loads, a missing one does not.
    #[test]
    fn relative_include_resolves_without_workspace_root_test() {
        use std::fs;

        let base = std::env::temp_dir().join(format!("ccls_confine_{}", std::process::id()));
        let ws = base.join("ws");
        let main_path = ws.join("main.circom");
        fs::create_dir_all(&ws).unwrap();
        fs::write(&main_path, "pragma circom 2.0.0;").unwrap();
        fs::write(ws.join("lib.circom"), "pragma circom 2.0.0;").unwrap();
        fs::write(base.join("sibling.circom"), "pragma circom 2.0.0;").unwrap();

        let main_url = Url::from_file_path(&main_path).unwrap();

        // No roots configured: a same-directory include still resolves (circom semantics).
        let mut no_roots = ContentCacheDb::new();
        assert!(
            no_roots.load_include(&main_url, "lib.circom").is_some(),
            "a relative include resolves even with no workspace roots"
        );

        // A `..` include to a sibling outside the root resolves too (circom allows `..`).
        let mut no_roots2 = ContentCacheDb::new();
        assert!(
            no_roots2
                .load_include(&main_url, "../sibling.circom")
                .is_some(),
            "a `..` include resolves relative to the source (circom allows it)"
        );

        // A non-existent include resolves to None (file simply isn't there).
        let mut db = ContentCacheDb::new();
        db.set_workspace_roots(vec![ws.canonicalize().unwrap()]);
        assert!(
            db.load_include(&main_url, "missing.circom").is_none(),
            "a missing include resolves to None"
        );
        // And a present same-directory include loads.
        assert!(
            db.load_include(&main_url, "lib.circom").is_some(),
            "a same-directory include loads"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
