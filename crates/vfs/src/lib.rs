//! Pure in-memory, path-interned virtual file system (rust-analyzer style).
//!
//! Responsibilities, all in-memory and free of any `lsp-types`/`rowan`/`syntax` dependency:
//! - **Path interning**: [`VfsPath`] (an *absolutized* [`PathBuf`]) <-> [`FileId`] (`u32` index).
//!   Two aliased paths absolutize to the same [`VfsPath`] -> same [`FileId`] (preserves the
//!   former path-hash guarantee, now via interning instead of hashing).
//! - **Text storage**: current [`Arc<str>`] per [`FileId`]; `None` text = absent/deleted.
//! - **Change log**: every mutation records a [`ChangedFile`]; drained via [`Vfs::take_changes`].
//!   This is the invalidation signal for the source db and the future hook for the file-watcher.
//!
//! No disk I/O happens here. Path resolution + `fs::read_to_string` stay in the LSP layer, which
//! feeds text in via [`Vfs::set_file_contents`]. Keeping vfs I/O-free makes it unit-testable
//! without touching the filesystem.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use path_absolutize::Absolutize;

/// Path-interned file identity. Two aliased paths share one id (they absolutize to one
/// [`VfsPath`]). Stable for the lifetime of the [`Vfs`] that allocated it.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FileId(pub u32);

/// An *absolutized* filesystem path, the interning key for [`FileId`]. Aliased paths
/// (`/a/../a/c` vs `/a/c`) collapse to one [`VfsPath`].
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct VfsPath(PathBuf);

impl VfsPath {
    /// Absolutize `path` into a [`VfsPath`]. Returns `None` if absolutization fails (e.g. a
    /// non-existent root on platforms that canonicalize). The caller (LSP layer) has already
    /// validated the path comes from a `file:` URL.
    pub fn from_abs_path(path: &Path) -> Option<Self> {
        let abs = path.absolutize().ok()?.to_path_buf();
        Some(VfsPath(abs))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// What kind of mutation a [`ChangedFile`] records. `Delete` is forward-looking for the
/// file-watcher (no deletions occur today) but idiomatic to include now.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
}

/// One recorded change to a file, drained by the source db to invalidate derived caches.
#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub file_id: FileId,
    pub change_kind: ChangeKind,
}

/// Per-file stored state: its absolutized path and current text (`None` = deleted/absent).
struct FileState {
    path: VfsPath,
    text: Option<Arc<str>>,
}

/// The in-memory virtual file system. Single-threaded by design: mutation takes `&mut self`.
/// (`RwLock`/a channel arrives only with the deferred file-watcher; the change-log surface is
/// identical either way, so this adds no rework.)
pub struct Vfs {
    path_to_id: HashMap<VfsPath, FileId>,
    files: Vec<FileState>,
    changes: Vec<ChangedFile>,
    /// Canonicalized workspace root folders. The containment primitive for confining untrusted
    /// `include "…"` resolution (path-traversal defense): an include may only resolve to a file
    /// inside one of these. Set once via [`Vfs::set_workspace_roots`]; empty (fail-closed) until
    /// then. The roots themselves are stored already-canonicalized by the caller, so
    /// [`Vfs::is_confined`] stays pure (no disk I/O) — the `canonicalize` stat is the LSP layer's
    /// job, keeping this crate I/O-free and unit-testable.
    workspace_roots: Vec<PathBuf>,
    /// Project-wide basename index: `file_name` (e.g. `lib.circom`) → every interned [`FileId`]
    /// with that basename. The search surface for include resolution when the same-dir lookup
    /// misses (an include whose target lives elsewhere in the project). Built by the LSP layer's
    /// workspace walk via [`Vfs::register_path`]; kept I/O-free here — only path identity, never a
    /// disk read. Ranking in [`Vfs::find_include`] is deterministic and pure.
    index: HashMap<String, Vec<FileId>>,
}

impl Default for Vfs {
    fn default() -> Self {
        Self::new()
    }
}

impl Vfs {
    pub fn new() -> Self {
        Self {
            path_to_id: HashMap::new(),
            files: Vec::new(),
            changes: Vec::new(),
            workspace_roots: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Set the workspace roots used by [`Self::is_confined`]. Callers (the LSP `initialize`
    /// handshake) pass already-canonicalized absolute paths; no disk I/O happens here.
    pub fn set_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        self.workspace_roots = roots;
    }

    /// The workspace roots confining [`Self::is_confined`] (already canonicalized by the caller).
    /// Read accessor so the LSP layer's workspace walker can re-walk the same roots it set.
    pub fn workspace_roots(&self) -> &[PathBuf] {
        &self.workspace_roots
    }

    /// Pure containment check: is `canonical` (an already-canonicalized absolute path) inside one of
    /// the workspace roots? **Fail-closed** — with no roots configured nothing is confined, so the
    /// LSP refuses to load any include rather than risk an arbitrary read. No disk I/O: the caller
    /// canonicalizes the candidate path (resolving `..`/symlinks) before calling.
    pub fn is_confined(&self, canonical: &Path) -> bool {
        !self.workspace_roots.is_empty()
            && self
                .workspace_roots
                .iter()
                .any(|root| canonical.starts_with(root))
    }

    /// The id for `path` if it has been interned, else `None`.
    #[inline]
    pub fn file_id(&self, path: &VfsPath) -> Option<FileId> {
        self.path_to_id.get(path).copied()
    }

    /// The absolutized path that allocated `id`, or `None` if unknown.
    pub fn path(&self, id: FileId) -> Option<&VfsPath> {
        self.files.get(id.0 as usize).map(|s| &s.path)
    }

    /// The current text of `id`, or `None` if the file is deleted/absent/unknown.
    #[inline]
    pub fn file_text(&self, id: FileId) -> Option<Arc<str>> {
        self.files.get(id.0 as usize).and_then(|s| s.text.clone())
    }

    /// Intern `path` (recording [`ChangeKind::Create`]) or update its text
    /// ([`ChangeKind::Modify`]). `text = None` deletes it ([`ChangeKind::Delete`]).
    ///
    /// **No-op when the text is unchanged**: pointer-equal `Arc<str>` short-circuits, falling
    /// back to a content compare. A no-op records *no* change and so triggers no downstream cache
    /// invalidation — a client resending identical content never forces a pointless reparse.
    /// Returns the (possibly newly allocated) id.
    pub fn set_file_contents(&mut self, path: VfsPath, text: Option<Arc<str>>) -> FileId {
        if let Some(&id) = self.path_to_id.get(&path) {
            let state = &mut self.files[id.0 as usize];
            let unchanged = match (&state.text, &text) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a.as_ref() == b.as_ref(),
                // Both None: nothing to do.
                (None, None) => true,
                _ => false,
            };
            if unchanged {
                return id;
            }
            let change_kind = match (&state.text, &text) {
                (Some(_), None) => ChangeKind::Delete,
                (None, Some(_)) => ChangeKind::Create,
                _ => ChangeKind::Modify,
            };
            state.text = text;
            self.changes.push(ChangedFile {
                file_id: id,
                change_kind,
            });
            id
        } else {
            // Unknown path: Create. A Create with None text is a no-op (no file to create).
            if text.is_none() {
                // No id allocated; pick a deterministic placeholder by interning empty? No —
                // instead just don't intern. But callers expect an id back. We intern the path
                // with absent text so the path is known, recording nothing (no content arrived).
                let id = FileId(self.files.len() as u32);
                self.files.push(FileState {
                    path: path.clone(),
                    text: None,
                });
                self.path_to_id.insert(path, id);
                return id;
            }
            let id = FileId(self.files.len() as u32);
            self.files.push(FileState {
                path: path.clone(),
                text,
            });
            self.path_to_id.insert(path, id);
            self.changes.push(ChangedFile {
                file_id: id,
                change_kind: ChangeKind::Create,
            });
            id
        }
    }

    /// Drain and return the pending change log. The source db consumes this to invalidate its
    /// derived caches.
    pub fn take_changes(&mut self) -> Vec<ChangedFile> {
        std::mem::take(&mut self.changes)
    }

    /// `true` if there are pending changes to drain.
    pub fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    // --- project-wide basename index (pure, no I/O) ---------------------------

    /// Intern `path` with **no text** and add it to the basename index, so a later
    /// [`Self::find_include`] can locate it. Idempotent: a path already interned (whether by an
    /// earlier `register_path` or by a text-bearing `set_file_contents`) keeps its existing
    /// [`FileId`] and is added to the index at most once. **Does not clobber loaded text** — if the
    /// path was loaded (e.g. by `load_include`), its text is preserved; only an unknown path is
    /// interned, and interning with `None` records no change-log entry (so no spurious cache drop).
    /// The caller canonicalizes the path *before* calling; this method stays pure.
    #[inline]
    pub fn register_path(&mut self, path: VfsPath) -> FileId {
        let id = if let Some(&id) = self.path_to_id.get(&path) {
            id
        } else {
            // Unknown path: intern with no text. This is the no-change-log branch of
            // `set_file_contents` (Create-with-None records nothing), so the path becomes known
            // without a disk read or a cache invalidation.
            self.set_file_contents(path.clone(), None)
        };
        if let Some(key) = file_name_of(path.as_path()) {
            push_dedup(self.index.entry(key).or_default(), id);
        }
        id
    }

    /// Remove `path`'s [`FileId`] from the basename index and drop any loaded text. Records a
    /// [`ChangeKind::Delete`] change **iff** the file had text, so the source db drops its
    /// parse/symbol-table caches — a delete must not leave stale caches behind. No-op (and no
    /// change recorded) if `path` was never interned, or was interned with no text.
    pub fn unregister_path(&mut self, path: &VfsPath) {
        let Some(&id) = self.path_to_id.get(path) else {
            return;
        };
        if let Some(key) = file_name_of(path.as_path()) {
            if let Some(vec) = self.index.get_mut(&key) {
                vec.retain(|f| *f != id);
                if vec.is_empty() {
                    self.index.remove(&key);
                }
            }
        }
        // Drop any loaded text. None→None is a no-op (no change); Some→None records a Delete.
        self.set_file_contents(path.clone(), None);
    }

    /// Pure ranked search for the best [`FileId`] matching include `rel` (e.g. `lib.circom` or
    /// `circuits/x.circom`) from the includer `parent` (any interned path, typically the includer
    /// file's own [`VfsPath`]). Candidates are every indexed file sharing `rel`'s basename. Ranking
    /// is deterministic:
    /// 1. **Suffix match** (desc): a candidate whose path *ends with* the full `rel` (so
    ///    `include "circuits/x.circom"` prefers `…/circuits/x.circom` over a bare `…/x.circom`).
    /// 2. **Nearest** (desc): longest shared path-component prefix with `parent`.
    /// 3. **Shortest path** (asc).
    /// 4. **Alphabetical** (asc) — the final tiebreak for full determinism.
    ///
    /// Returns `None` if the basename isn't indexed.
    #[inline]
    pub fn find_include(&self, parent: &VfsPath, rel: &str) -> Option<FileId> {
        let rel_path = Path::new(rel);
        let key = file_name_of(rel_path)?;
        let candidates = self.index.get(&key)?;
        if candidates.is_empty() {
            return None;
        }
        let parent_path = parent.as_path();
        candidates.iter().copied().min_by_key(|&id| {
            let cpath = self
                .path(id)
                .map(|p| p.as_path())
                .unwrap_or_else(|| Path::new(""));
            // min_by_key picks the smallest key, so the "want-highest" fields are inverted. The
            // final tiebreak compares the path's `OsStr` directly (`OsStr: Ord`, allocation-free,
            // and MSRV-safe — unlike `as_encoded_bytes` which needs Rust 1.74).
            let osname = cpath.as_os_str();
            (
                !cpath.ends_with(rel_path),
                Reverse(shared_component_prefix(parent_path, cpath)),
                osname.len(),
                osname,
            )
        })
    }
}

/// The `file_name` of `path` as a UTF-8 `String` (the index key), or `None` if it has none or is
/// non-UTF-8. Both registration and lookup derive the key the same way, so a `&str` include and an
/// interned path agree on their basename bucket.
fn file_name_of(path: &Path) -> Option<String> {
    path.file_name()?.to_str().map(|s| s.to_string())
}

/// Push `id` into `vec` only if absent (keeps the index dedup'd under repeated registration).
fn push_dedup(vec: &mut Vec<FileId>, id: FileId) {
    if !vec.contains(&id) {
        vec.push(id);
    }
}

/// Count of leading path components `candidate` shares with `parent` (component-wise, not
/// byte-wise). Used by [`Vfs::find_include`] to prefer the include target nearest the includer.
/// Zips the two paths' component iterators directly — no intermediate `Vec` allocation.
fn shared_component_prefix(parent: &Path, candidate: &Path) -> usize {
    parent
        .components()
        .zip(candidate.components())
        .take_while(|(a, b)| a == b)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(s: &str) -> VfsPath {
        VfsPath::from_abs_path(Path::new(s)).unwrap()
    }

    #[test]
    fn path_aliasing_collapses_to_one_id_test() {
        // `/a/../a/c` and `/a/c` absolutize to the same path -> same FileId.
        let mut vfs = Vfs::new();
        let id1 = vfs.set_file_contents(vp("/a/../a/c"), Some(Arc::from("x")));
        let id2 = vfs.file_id(&vp("/a/c"));
        assert_eq!(Some(id1), id2, "aliased paths must share one FileId");
    }

    #[test]
    fn create_then_modify_recorded_test() {
        let mut vfs = Vfs::new();
        let p = vp("/m.txt");
        vfs.set_file_contents(p.clone(), Some(Arc::from("a")));
        vfs.set_file_contents(p.clone(), Some(Arc::from("b")));
        let changes = vfs.take_changes();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].change_kind, ChangeKind::Create);
        assert_eq!(changes[1].change_kind, ChangeKind::Modify);
    }

    #[test]
    fn take_changes_drains_and_has_changes_flips_test() {
        let mut vfs = Vfs::new();
        let p = vp("/d.txt");
        vfs.set_file_contents(p.clone(), Some(Arc::from("a")));
        assert!(vfs.has_changes());
        let drained = vfs.take_changes();
        assert_eq!(drained.len(), 1);
        assert!(!vfs.has_changes(), "draining clears the change log");
    }

    #[test]
    fn file_text_round_trips_test() {
        let mut vfs = Vfs::new();
        let p = vp("/r.txt");
        let id = vfs.set_file_contents(p.clone(), Some(Arc::from("hello")));
        assert_eq!(vfs.file_text(id).as_deref(), Some("hello"));
    }

    #[test]
    fn identical_text_records_no_change_test() {
        let mut vfs = Vfs::new();
        let p = vp("/same.txt");
        let text: Arc<str> = Arc::from("unchanged");
        vfs.set_file_contents(p.clone(), Some(text.clone()));
        // _ = drain so the second call starts from an empty change log.
        let _ = vfs.take_changes();
        let id = vfs.set_file_contents(p.clone(), Some(text));
        assert!(
            !vfs.has_changes(),
            "re-submitting identical text must record no change"
        );
        // Content-equal but not pointer-equal must also be a no-op.
        let id2 = vfs.set_file_contents(p.clone(), Some(Arc::from("unchanged")));
        assert_eq!(id, id2);
        assert!(!vfs.has_changes(), "content-equal text is also a no-op");
    }

    #[test]
    fn none_text_is_delete_test() {
        let mut vfs = Vfs::new();
        let p = vp("/del.txt");
        let id = vfs.set_file_contents(p.clone(), Some(Arc::from("a")));
        let _ = vfs.take_changes();
        vfs.set_file_contents(p.clone(), None);
        let changes = vfs.take_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_kind, ChangeKind::Delete);
        assert_eq!(vfs.file_text(id), None, "deleted file has no text");
    }

    // --- basename index tests (pure, no disk) ---------------------------------

    #[test]
    fn register_then_find_include_test() {
        let mut vfs = Vfs::new();
        let a = vfs.register_path(vp("/proj/a/lib.circom"));
        let b = vfs.register_path(vp("/proj/b/lib.circom"));
        let parent = vp("/proj/main.circom");
        // Same suffix (no), equal shared prefix (/proj), equal length → alphabetical: a < b.
        assert_eq!(vfs.find_include(&parent, "lib.circom"), Some(a));
        let _ = b;
    }

    #[test]
    fn find_include_suffix_match_preferred_test() {
        let mut vfs = Vfs::new();
        let bare = vfs.register_path(vp("/proj/x.circom"));
        let nested = vfs.register_path(vp("/proj/circuits/x.circom"));
        let parent = vp("/proj/main.circom");
        // `circuits/x.circom` matches the nested candidate's suffix; bare does not.
        assert_eq!(vfs.find_include(&parent, "circuits/x.circom"), Some(nested));
        // `x.circom` suffix-matches both; tie on shared prefix; shortest path wins → bare.
        assert_eq!(vfs.find_include(&parent, "x.circom"), Some(bare));
    }

    #[test]
    fn find_include_nearest_wins_test() {
        let mut vfs = Vfs::new();
        let near = vfs.register_path(vp("/proj/sub/lib.circom"));
        let _far = vfs.register_path(vp("/other/lib.circom"));
        let parent = vp("/proj/sub/main.circom");
        // `near` shares `/proj/sub` with the includer; `far` shares only `/`.
        assert_eq!(vfs.find_include(&parent, "lib.circom"), Some(near));
    }

    #[test]
    fn register_path_idempotent_test() {
        let mut vfs = Vfs::new();
        let p = vp("/proj/lib.circom");
        let id1 = vfs.register_path(p.clone());
        let id2 = vfs.register_path(p.clone());
        assert_eq!(id1, id2, "re-registering the same path returns one id");
        assert_eq!(
            vfs.find_include(&vp("/proj/m.circom"), "lib.circom"),
            Some(id1)
        );
    }

    #[test]
    fn register_path_preserves_loaded_text_test() {
        let mut vfs = Vfs::new();
        let p = vp("/proj/lib.circom");
        let id = vfs.set_file_contents(p.clone(), Some(Arc::from("loaded")));
        let _ = vfs.take_changes();
        // Registering an already-loaded path must NOT delete its text or record a change.
        let id2 = vfs.register_path(p.clone());
        assert_eq!(id, id2);
        assert_eq!(vfs.file_text(id).as_deref(), Some("loaded"));
        assert!(
            !vfs.has_changes(),
            "registering a loaded path records no change"
        );
    }

    #[test]
    fn unregister_drops_candidate_test() {
        let mut vfs = Vfs::new();
        let p = vp("/proj/lib.circom");
        let id = vfs.register_path(p.clone());
        assert!(vfs
            .find_include(&vp("/proj/m.circom"), "lib.circom")
            .is_some());
        // A path-only file (never loaded) has no text to delete → no change recorded.
        vfs.unregister_path(&p);
        assert!(
            !vfs.has_changes(),
            "deleting a never-loaded path records no change"
        );
        assert!(vfs
            .find_include(&vp("/proj/m.circom"), "lib.circom")
            .is_none());
        // Idempotent: unregistering again is a no-op.
        vfs.unregister_path(&p);
        let _ = id;
    }

    #[test]
    fn unregister_loaded_records_delete_test() {
        let mut vfs = Vfs::new();
        let p = vp("/proj/lib.circom");
        let id = vfs.set_file_contents(p.clone(), Some(Arc::from("loaded")));
        vfs.register_path(p.clone());
        let _ = vfs.take_changes();
        // Deleting a loaded file must record a Delete so caches drop.
        vfs.unregister_path(&p);
        let changes = vfs.take_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_kind, ChangeKind::Delete);
        assert_eq!(changes[0].file_id, id);
        assert!(vfs.file_text(id).is_none());
    }
}
