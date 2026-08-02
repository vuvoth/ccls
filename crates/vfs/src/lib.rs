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
        }
    }

    /// Set the workspace roots used by [`Self::is_confined`]. Callers (the LSP `initialize`
    /// handshake) pass already-canonicalized absolute paths; no disk I/O happens here.
    pub fn set_workspace_roots(&mut self, roots: Vec<PathBuf>) {
        self.workspace_roots = roots;
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
    pub fn file_id(&self, path: &VfsPath) -> Option<FileId> {
        self.path_to_id.get(path).copied()
    }

    /// The absolutized path that allocated `id`, or `None` if unknown.
    pub fn path(&self, id: FileId) -> Option<&VfsPath> {
        self.files.get(id.0 as usize).map(|s| &s.path)
    }

    /// The current text of `id`, or `None` if the file is deleted/absent/unknown.
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
}
