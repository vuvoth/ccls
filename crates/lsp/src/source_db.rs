//! Salsa-shaped source database over a real in-memory [`vfs::Vfs`].
//!
//! This is the foundation for [`crate::global_state::GlobalState`]. Each method of
//! [`SourceDatabase`] is designed to map 1:1 onto a salsa `#[salsa::input]` / `#[salsa::tracked]`
//! query, so adopting salsa later is an implementation swap rather than a redesign.
//!
//! Responsibilities are split cleanly (rust-analyzer style):
//! - **[`vfs::Vfs`]** owns "what files exist and what's their text" + a change log.
//! - **[`ContentCacheDb`]** owns "what we derived from that text" (parse tree, file DB, symbol
//!   table), invalidating those caches by *draining the VFS change log*.
//!
//! The two guarantees this preserves (see the plan):
//! 1. `include`s are read from disk + re-parsed **once** and then served from cache, instead of on
//!    every keystroke of the main file.
//! 2. A client resending unchanged text records no change, so it never blows the caches or forces a
//!    pointless reparse.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use lsp_types::Url;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::syntax::syntax_tree;
use syntax::syntax_node::SyntaxNode;
use vfs::{ChangedFile, Vfs, VfsPath};

use crate::file_db::{FileDB, FileId};
use crate::semantic::SymbolTable;

/// Source-level queries over a set of open files. Every method is keyed by [`FileId`] and either
/// returns an input (`file_text`) or a content-derived value (`parse`/`ast`/`file_db`/
/// `symbol_table`).
///
/// All methods take `&self`: salsa queries are `&self` with memoization handled by the runtime, and
/// the cache impl below mirrors that via interior mutability. Callers always receive owned values,
/// so no borrow is held across a query.
pub trait SourceDatabase {
    /// The full text of `id`. This is the single `#[salsa::input]`; everything else is derived.
    fn file_text(&self, id: FileId) -> Arc<str>;
    /// The syntax tree for `id`'s text (memoized).
    fn parse(&self, id: FileId) -> SyntaxNode;
    /// The top-level AST node, or `None` if the parse produced no `CircomProgram` root.
    fn ast(&self, id: FileId) -> Option<AstCircomProgram>;
    /// Offset/line bookkeeping for `id` (memoized).
    fn file_db(&self, id: FileId) -> FileDB;
    /// The lexical symbol table for `id` (memoized). Built lazily on first query from the cached
    /// parse, and dropped by the change-log invalidation on any edit of `id`.
    fn symbol_table(&self, id: FileId) -> Arc<SymbolTable>;
}

/// All lazily-computed caches live behind a single `RefCell` so a query method only ever takes one
/// short-lived borrow (read for the hit check, write for the miss fill) — never nested.
///
/// Invalidation is change-log-driven (eager): a mutation drains [`Vfs::take_changes`] and removes
/// only the changed ids from each cache, so editing file A never recomputes file B's cache. There
/// is no per-entry content hash — the change log is the single invalidation signal.
struct Caches {
    parse: HashMap<FileId, SyntaxNode>,
    file_db: HashMap<FileId, FileDB>,
    symbol_table: HashMap<FileId, Arc<SymbolTable>>,
    /// Number of actual (cache-miss) parses per file. Test-only observable proving memoization.
    /// Not touched by invalidation: it counts total parses, not cached vs. uncached.
    parse_count: HashMap<FileId, usize>,
}

/// [`SourceDatabase`] backed by a [`Vfs`] + derived caches. The server is single-threaded, so
/// `RefCell` is sufficient for the cache fills behind `&self` query methods.
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

    /// Convert a `file:`-scheme URL to its absolutized [`VfsPath`]. Returns `None` for non-`file:`
    /// schemes (untitled/git docs) or paths that can't be absolutized. Single source of truth for
    /// the URI→path step so interning and lookup can never disagree (a split here would intern
    /// under one key and look up under another, silently breaking resolution).
    fn url_to_vpath(url: &Url) -> Option<VfsPath> {
        let path = url.to_file_path().ok()?;
        VfsPath::from_abs_path(&path)
    }

    /// Resolve a relative include `rel` against `parent_url`'s directory to an absolutized
    /// [`VfsPath`]. Shared by [`Self::load_include`] (load-or-serve), [`Self::id_for_include`]
    /// (serve-only), and the goto-definition path so all three agree on the resolved path — an
    /// include `"../lib.circom"` resolves to the same canonical path whether it's being loaded,
    /// looked up, or jumped to.
    fn resolve_include(parent_url: &Url, rel: &str) -> Option<VfsPath> {
        let parent_path = parent_url.to_file_path().ok()?;
        let parent_dir = parent_path.parent()?;
        let lib_path = parent_dir.join(rel);
        VfsPath::from_abs_path(&lib_path)
    }

    /// The `FileId` for `url`, computed only from its absolutized path so aliased paths
    /// (`/a/../a/c` vs `/a/c`) collapse to one id via interning. Returns `None` for non-`file:`
    /// schemes (untitled/git docs) or paths that can't be absolutized — callers skip indexing
    /// those.
    pub fn id_for_url(&self, url: &Url) -> Option<FileId> {
        self.vfs.file_id(&Self::url_to_vpath(url)?)
    }

    /// The already-interned `FileId` for an include, without reading disk. Returns `None` if the
    /// path can't be resolved or the include hasn't been loaded yet. Used by cross-file
    /// goto-definition (the include is loaded once in `handle_update`).
    pub fn id_for_include(&self, parent_url: &Url, rel: &str) -> Option<FileId> {
        let vpath = Self::resolve_include(parent_url, rel)?;
        self.vfs.file_id(&vpath)
    }

    /// Register a document or update its text. Returns `(FileId, changed)`: `changed` is `false`
    /// when the text was identical to what's already stored (a no-op — no change recorded, no
    /// cache drop), letting the caller skip pointless reindexing on a client resending unchanged
    /// content. Returns `None` if `url` is non-`file:`.
    pub fn set_document(&mut self, url: &Url, text: String) -> Option<(FileId, bool)> {
        let vpath = Self::url_to_vpath(url)?;
        let id = self.vfs.set_file_contents(vpath, Some(Arc::from(text)));
        let changes = self.invalidate_changed();
        let changed = changes.iter().any(|c| c.file_id == id);
        Some((id, changed))
    }

    /// Resolve a relative include `rel` against `parent_url`'s directory and load it from disk
    /// **once**. On every subsequent call the already-interned `FileId` is returned without
    /// touching the filesystem — so a keystroke in the main file never re-reads its includes.
    ///
    /// Returns `None` (and is skipped) for non-`file:` schemes, a missing parent dir, an unreadable
    /// file, or a path that can't be absolutized.
    pub fn load_include(&mut self, parent_url: &Url, rel: &str) -> Option<FileId> {
        let vpath = Self::resolve_include(parent_url, rel)?;

        // Already loaded — serve the cached id, never re-read on the main file's keystroke.
        if let Some(id) = self.vfs.file_id(&vpath) {
            return Some(id);
        }

        let src = std::fs::read_to_string(vpath.as_path()).ok()?;
        let id = self.vfs.set_file_contents(vpath, Some(Arc::from(src)));
        self.invalidate_changed();
        Some(id)
    }

    /// Drain the VFS change log and drop every changed id's derived caches (other files are
    /// untouched). `parse_count` is intentionally preserved — it tracks total parses, not cache
    /// state. Returns the drained changes so callers can tell whether a specific id changed.
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

    /// Reconstruct the `file:` URL for `id` from its absolutized VFS path. Safe because `id` was
    /// interned from a `file:`-scheme URL at registration time.
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
        // Hit check under a short-lived shared borrow; the guard is dropped at the end of the block.
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
        // `id` and `url` were both validated as `file:`-scheme at registration time, so `new`
        // (which skips re-hashing) is safe here.
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

        // Lazy Phase B index: build the per-file `SymbolTable` from the cached parse + file DB.
        // A file whose text fails to parse to a program root yields an empty table (no `unwrap`).
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
        // Non-empty: the file top-level carries the `Multiplier2` template, and its body has
        // params/signals. Member counts are the real correctness signal here.
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
}
