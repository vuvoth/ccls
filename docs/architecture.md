# Architecture

CCLS is a multi-crate Rust workspace plus a TypeScript VS Code extension. This page describes
the workspace layout and the design of the three load-bearing subsystems: the **resolution
core**, the **source database**, and the **virtual file system** (including sandboxed
includes).

## Workspace layout

```
circom-language-server/
├── crates/
│   ├── parser/    # `logos` lexer + event-driven parser with markers
│   ├── syntax/    # `rowan` lossless syntax tree + typed AST
│   ├── vfs/       # Virtual file system (existence, text, change log)
│   └── lsp/       # LSP server: handlers, global state, resolver, semantic index
├── editors/code/  # VS Code extension (TypeScript, `circom-plus`)
└── xtask/         # Build & install tasks (`cargo xtask install …`)
```

See [crates/](./crates/) for per-crate deep dives.

## Server entry point

`crates/lsp/src/main.rs` opens a stdio LSP connection, advertises
[`server_capabilities`](https://microsoft.github.io/language-server-protocol/) (definition,
hover, completion, references, documentSymbol, formatting, rename with `prepareSupport`), and
runs a `main_loop` that routes each `Message::Request` to `GlobalState::handle_request` and
each `Message::Notification` to `GlobalState::handle_notification`. Workspace roots are
captured from the `initialize` handshake (modern `workspace_folders`, falling back to legacy
`root_uri`), canonicalized, and handed to the VFS for include confinement.

## Resolution core

The resolution core lives in `crates/lsp/src/resolver.rs` and
`crates/lsp/src/symbol_table.rs`. It is the single name-resolution engine shared by
goto-definition, hover, references, and rename — so a given symbol resolves identically across
all features.

- **`SymbolTable`** (`symbol_table.rs`) is a per-file, scope-aware index of declarations by
  name: the file top-level (template/function/bus names) plus each body scope
  (params/signals/vars/components). Each `Symbol` carries a `def_range` (the **name token** —
  what goto-def/rename/references jump to) and a `decl_range` (the **whole declaration** —
  what hover shows), plus a `type_name` (the instantiated template, for `component c = T();`).
- **`resolver`** (`resolver.rs`) is pure: given a token under the cursor and a file's
  `SymbolTable`, it returns the `ResolvedSymbol`s the token refers to. It owns no I/O and no
  LSP request types — callers map each `def_range` into a file-scoped `lsp_types::Location`.

Resolution is **purely name-based** against the table. This is what makes it sound across
files: the same name resolves in each file's own table without reusing a foreign token's
identity (the legacy `hash(text)` index was structurally unsound for cross-file lookups).

## Source database

`crates/lsp/src/source_db.rs` is a salsa-shaped source database layered over the in-memory
[`Vfs`](#virtual-file-system-vfs). Each `SourceDatabase` method maps 1:1 to a future salsa
query, so adopting salsa later is an implementation swap, not a redesign.

The split (rust-analyzer style):

- [`Vfs`](./crates/vfs.md) owns **file existence + text + change log**.
- `ContentCacheDb` owns **derived caches** — parse, AST, `FileDB` (offset/line bookkeeping),
  and `SymbolTable` — keyed by `FileId`, behind a single `RefCell`.

### Change-log-driven invalidation

Invalidation is driven by **draining the VFS change log**, not by content hashing. When a query
runs, the DB drains `Vfs::take_changes()` and drops only the caches for changed ids. The
guarantees this gives:

- **Editing file A never recomputes file B** — only changed `FileId`s are invalidated.
- Includes are read from disk and parsed **once**, then cached.
- A client resending **identical text records no change** (pointer-equal then content-equal
  short-circuit in the VFS) and so triggers no reparse.

## Virtual file system (VFS)

`crates/vfs/src/lib.rs` is a pure in-memory, path-interned VFS in the rust-analyzer style.
It deliberately does **no disk I/O** — path resolution and `fs::read_to_string` stay in the
LSP layer, which feeds text in via `Vfs::set_file_contents`. Keeping the VFS I/O-free makes it
unit-testable without touching the filesystem.

Responsibilities:

- **Path interning**: an absolutized `VfsPath` ↔ `FileId` (`u32`). Aliased paths
  (`/a/../a/c` vs `/a/c`) absolutize to one `VfsPath` → one `FileId`.
- **Text storage**: a current `Arc<str>` per `FileId`; `None` text means absent/deleted.
- **Change log**: every mutation records a `ChangedFile` (`Create`/`Modify`/`Delete`), drained
  via `Vfs::take_changes()` — the invalidation signal for the source DB and the future hook
  for a file-watcher.

See [crates/vfs.md](./crates/vfs.md) for the full API.

## Include resolution

`include "…"` resolution follows circom semantics: the path is resolved **relative to the
including source file**, the way the circom compiler resolves it. It is **not** confined to the
workspace roots.

- `Vfs::set_workspace_roots` stores canonicalized root paths from the `initialize` handshake.
  They scope the project `.circom` walk that feeds the basename index — an *indexing* scope, not
  a confinement gate.
- An **absolute** include path is refused (`source_db::resolve_include`): `PathBuf::join` would
  otherwise replace the base (`include "/etc/passwd"`), enabling an arbitrary local-file read.
- Relative and `..` includes resolve normally and **may read files outside the workspace roots**,
  matching circom — so navigation works even when the editor points at the wrong/incomplete folder.
- The basename fallback (`Vfs::find_include`) only returns files the workspace walk already
  indexed, so that path cannot escape the indexed set.

## Extension

The TypeScript client in `editors/code/` (`circom-plus`) is a thin LSP client: it spawns the
server binary, forwards the standard text-document requests/notifications, and wires the
results into VS Code's goto/hover/completion/references/rename UI. See
[`editors/code/README.md`](../editors/code/README.md).
