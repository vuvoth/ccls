# `lsp` crate

The Language Server Protocol implementation, built on
[`lsp-server`](https://docs.rs/lsp-server) and
[`lsp-types`](https://docs.rs/lsp-types). This is the crate that becomes the `ccls` server
binary; it owns document state, request dispatch, and the resolution/index machinery that all
features ride on.

> Also see [`architecture.md`](../architecture.md) for how the pieces fit together, and
> [`features.md`](../features.md) for the user-facing feature list.

## Layout

```
crates/lsp/src/
├── main.rs            # binary entry: stdio connection, capabilities, main loop
├── global_state.rs    # GlobalState: document state + request/notification dispatch
├── handler.rs         # handler module registry (one module per feature)
├── handler/
│   ├── goto_definition.rs   # textDocument/definition          (implemented)
│   ├── hover.rs             # textDocument/hover                (implemented)
│   ├── completion.rs        # textDocument/completion           (implemented)
│   ├── references.rs        # textDocument/references           (implemented)
│   ├── rename.rs            # textDocument/rename (+ prepare)   (implemented)
│   ├── document_symbol.rs   # textDocument/documentSymbol       (placeholder: returns None)
│   └── formatting.rs        # textDocument/formatting           (placeholder: returns None)
├── resolver.rs        # pure token -> definition resolver (shared resolution core)
├── symbol_table.rs    # per-file, scope-aware declaration index
├── source_db.rs       # salsa-shaped source DB over the Vfs (parse/ast/file_db/symbol_table)
└── file_db.rs         # per-file offset/line bookkeeping + FileId re-export
```

## Entry point (`main.rs`)

`main()` opens a stdio `Connection`, serializes `server_capabilities()`, runs the
`initialize` handshake, and enters `main_loop`. **All logging goes to stderr** — stdout is
the LSP message channel.

`server_capabilities()` advertises:

- `text_document_sync: FULL` (full document on every change — incremental sync is a roadmap
  item).
- `definition`, `hover`, `references`, `documentSymbol`, `documentFormatting` providers.
- `completion` with the `.` trigger character.
- `rename` with `prepareSupport` (the client consults the server before opening the rename
  box, so keywords/strings never become renamable).

`workspace_roots(params)` extracts workspace folders (modern `workspace_folders`, falling
back to legacy `root_uri`), converts each from a `file:` URI to a path, and canonicalizes
them. These roots feed the VFS's include-confinement check.

## Main loop

`main_loop` reads `Message`s off the connection:

- `Message::Request` — if it isn't `shutdown` (handled by the transport), routed to
  `GlobalState::handle_request`, which dispatches to the matching `handler::*::handle` by
  request type and sends back a `Message::Response`.
- `Message::Notification` — routed to `GlobalState::handle_notification` (e.g.
  `didOpen` / `didChange`, which update document text in the source DB).
- `Message::Response` — ignored (the server sends no requests of its own).

## Global state (`global_state.rs`)

`GlobalState` holds the `ContentCacheDb` (the source DB) and provides the dispatch surface.
It also exposes the shared cursor prologue every read-handler uses — `cursor_context(uri,
position)` resolves the URL to a `FileId`, fetches the AST and `FileDB`, and converts the LSP
position to a byte offset, returning a `CursorContext { id, ast, file_db, offset }`.

The handler contract (see `handler.rs`): each feature module exposes
`fn handle(state: &GlobalState, params: P) -> Result<Option<R>>`.

> Note: the `handler.rs` module doc-comment currently says "the rest are placeholders", but
> `hover`, `completion`, `references`, and `rename` are in fact fully implemented (only
> `document_symbol` and `formatting` remain placeholders). Trust the individual handler files.

## Handlers (`handler/`)

Each handler rides the shared resolution core. Highlights:

- **`goto_definition`** — resolves signals/vars/params/templates/functions/components, with
  cross-file jumps via `include` and a `jump_to_lib` path for include-path strings.
- **`hover`** — kind + declaration signature (header only for block-bodied defs).
- **`completion`** — in-scope names, top-level names, keywords, and member completion
  (`c.<signal>`) that resolves a component's template across files.
- **`references`** / **`rename`** — occurrences found by *resolving* candidates (not
  text-matching), so shadowing is correct; both are in-file today (cross-file is a roadmap
  item).

## Resolution core (`resolver.rs` + `symbol_table.rs`)

- `SymbolTable` (`symbol_table.rs`) — per-file index of declarations by name: the file
  top-level plus each body scope. A `Symbol` carries `def_range` (name token), `decl_range`
  (whole declaration), and `type_name` (the instantiated template for components).
- `resolver` (`resolver.rs`) — pure token → `ResolvedSymbol` lookup over a `SymbolTable`.
  Shared helpers: `token_at_offset`, `identifier_at`, `token_ancestors`, `component_field`.

See [`architecture.md` § Resolution core](../architecture.md#resolution-core).

## Source DB (`source_db.rs`)

`SourceDatabase` is a salsa-shaped trait (`file_text` is the input; `parse`/`ast`/`file_db`/
`symbol_table` are derived, memoized via interior mutability). `ContentCacheDb` is the
`Vfs`-backed implementation. Invalidation is change-log-driven: draining
`Vfs::take_changes()` drops only changed ids, so editing file A never recomputes file B. See
[`architecture.md` § Source database](../architecture.md#source-database).

## File DB (`file_db.rs`)

`FileDB` is per-file offset/line bookkeeping: the `FileId` (re-exported from `vfs`), the
`file:` URL, the byte offset of every `\n`, and the source text. It converts between LSP
positions (UTF-16 code-unit counts) and byte offsets — the bridge between the protocol and
rowan's byte-based `TextSize` ranges.

## Tests

Handler and resolver tests use `insta` snapshots under `crates/lsp/src/handler/snapshots/`
and fixtures under `crates/lsp/src/test_files/` (including a `with_include/` cross-file
case). Run with `cargo test -p lsp`.
