# `vfs` crate

A pure in-memory, **path-interned** virtual file system in the rust-analyzer style. It is the
single source of truth for file existence and text, and it emits the change log that drives
cache invalidation in the [`lsp`](./lsp.md) source DB.

This crate does **no disk I/O** — by design. Path resolution and `fs::read_to_string` stay in
the LSP layer, which feeds text in via `Vfs::set_file_contents`. Keeping the VFS I/O-free
makes it unit-testable without touching the filesystem.

Source: `crates/vfs/src/lib.rs` (the whole crate is one file).

## Core types

- **`FileId(u32)`** — path-interned file identity. Two aliased paths share one id because
  they absolutize to one `VfsPath`. Stable for the lifetime of the `Vfs` that allocated it.
- **`VfsPath(PathBuf)`** — an *absolutized* path, the interning key. Aliased paths
  (`/a/../a/c` vs `/a/c`) collapse to one `VfsPath` (built via `path-absolutize`).
- **`ChangeKind`** — `Create` / `Modify` / `Delete`.
- **`ChangedFile { file_id, change_kind }`** — one recorded change, drained by the source DB.
- **`Vfs`** — the store: `path_to_id` map, a `Vec<FileState>` (path + `Option<Arc<str>>`
  text), a pending change log, and canonicalized workspace roots. Single-threaded by design
  (mutation takes `&mut self`); a `RwLock`/channel arrives only with the deferred
  file-watcher, and the change-log surface is identical either way.

## API surface

| Method | Purpose |
| --- | --- |
| `VfsPath::from_abs_path(path)` | Absolutize a `&Path` into a `VfsPath` (`None` if it fails). |
| `Vfs::set_workspace_roots(roots)` | Store canonicalized workspace roots (called at `initialize`). |
| `Vfs::is_confined(canonical)` | **Pure**, fail-closed containment check (no disk I/O). |
| `Vfs::set_file_contents(path, text)` | Intern/update/delete a file; records a `ChangedFile`. |
| `Vfs::take_changes()` | Drain and return the pending change log. |
| `Vfs::has_changes()` | `true` if there are pending changes. |
| `Vfs::file_id(path)` / `Vfs::path(id)` / `Vfs::file_text(id)` | Identity + text lookups. |

## Key behaviors

- **No-op on identical text** — `set_file_contents` short-circuits when the new text is
  pointer-equal (`Arc::ptr_eq`) and falls back to a content compare. A no-op records *no*
  change, so a client resending identical content triggers no downstream invalidation.
- **`None` text = delete** — setting `text = None` on an existing file records a `Delete`
  (forward-looking for the file-watcher; no deletions occur today).
- **Change log as the invalidation signal** — the source DB drains `take_changes()` to drop
  only the caches for changed ids. There is no per-entry content hash; the change log is the
  only signal.

## Sandboxed includes

`is_confined` is the path-traversal defense for `include "…"` resolution:

- The LSP layer canonicalizes a candidate include path (resolving `..` and symlinks) before
  calling, then checks `is_confined`.
- `is_confined` compares `canonical.starts_with(root)` against the stored roots — pure, no
  disk I/O.
- **Fail-closed**: with no roots configured, nothing is confined, so the server refuses to
  load any include rather than risk an arbitrary read.

## Tests

`lib.rs` includes unit tests covering path aliasing, create/modify recording, change-log
draining, text round-tripping, the identical-text no-op, and `None`-as-delete. Run with
`cargo test -p vfs`.
