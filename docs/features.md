# Features

What CCLS provides today, and how each feature resolves symbols under the hood. The roadmap
of features **not yet** implemented lives in [roadmap.md](./roadmap.md).

All features below share a single **resolution core** (see
[architecture.md](./architecture.md)): a name-based resolver over a per-file symbol table.
That means the same symbol resolves consistently whether you go-to-definition, hover,
find-references, or rename it.

## Implemented

### Go to Definition

Resolves signals, variables, parameters, templates, functions, and components — including
**cross-file** jumps through `include` statements, and jumping straight into an included
library file from its `"path.circom"` string.

- Handler: `crates/lsp/src/handler/goto_definition.rs`
- It uses `token_at_offset` (not `identifier_at`) because an include-path `CircomString` is
  also a valid jump target.

### Hover

Shows the symbol kind and its declaration signature (header only for block-bodied definitions
like `template` / `function` / `bus`).

- Handler: `crates/lsp/src/handler/hover.rs`
- Member-access fields (`c.x`) are not resolved by the flat resolver and yield `None`
  (consistent with rename/references).

### Completion

In-scope body symbols, file top-level names, reserved keywords, and **member completion**
(`component.<signal>`) that resolves a component's template across files.

- Handler: `crates/lsp/src/handler/completion.rs`
- Member mode detects `receiver.<partial>`, resolves the receiver's template (in-file or via
  `include`), and offers that template's signals.
- Normal mode returns `is_incomplete: false` — the client filters by the typed word.
- The `.` trigger character is advertised in server capabilities.

### Find References

Every occurrence of a symbol, resolved *semantically* (not text-matched), so shadowing is
respected.

- Handler: `crates/lsp/src/handler/references.rs`
- Returns the declaration plus every in-scope use as `Location`s. **In-file by design** —
  cross-file references are a follow-up (see roadmap).

### Rename

Scope-aware rename with `prepareRename` support; refuses keywords, include-path strings,
illegal names, and unresolved member-access fields.

- Handler: `crates/lsp/src/handler/rename.rs`
- Occurrences are found by *resolving* each candidate (not text-matching), so shadowing is
  correct. In-file (the symbol's defining file); cross-file rename is a follow-up.
- `prepareSupport` is advertised so the client consults the server (not its own textual word
  check) before opening the rename box.

### Error-recovering parser

Keeps working on invalid or partially-typed circom files. See
[crates/parser.md](./crates/parser.md) for the marker-based recovery model.

### Lazy, cached analysis

Parsing and symbol tables are memoized and invalidated only on real edits; includes are read
from disk once. A client resending identical text records no change and triggers no reparse.
See [architecture.md](./architecture.md#source-database) for the change-log-driven
invalidation.

### Sandboxed includes

`include` resolution is confined to workspace roots — path-traversal and symlink-safe. With no
roots configured the server refuses to load any include rather than risk an arbitrary file
read (fail-closed). See [architecture.md](./architecture.md#sandboxed-includes).

## Registered but not yet implemented

Two handlers are advertised in server capabilities but currently return an empty result. See
[roadmap.md](./roadmap.md) for details:

- **Document Symbol / Outline** — `crates/lsp/src/handler/document_symbol.rs`
- **Formatting** — `crates/lsp/src/handler/formatting.rs`
