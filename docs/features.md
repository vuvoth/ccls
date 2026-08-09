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

### Go to Implementation

Behaves identically to Go to Definition — circom has no separate implementation concept (no
interfaces/traits distinct from a definition). Delegates to the definition handler so the two
cannot drift.

- Handler: `crates/lsp/src/handler/goto_implementation.rs`

### Workspace Symbol

`workspace/symbol` lists every top-level template/function/bus across the workspace matching the
query (empty query ⇒ all), powered by the cached per-file symbol tables.

- Handler: `crates/lsp/src/handler/workspace_symbol.rs`

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

Every occurrence of a symbol across the workspace, resolved *semantically* (not text-matched), so
shadowing and same-name collisions across files are respected.

- Handler: `crates/lsp/src/handler/references.rs`
- Returns the declaration plus every reference as `Location`s. Workspace-wide: scans only files
  that could reference the target (via the cached identifier index + include visibility).

### Rename

Scope-aware rename with `prepareRename` support; refuses keywords, include-path strings,
illegal names, and unresolved member-access fields.

- Handler: `crates/lsp/src/handler/rename.rs`
- Occurrences are found by *resolving* each candidate (not text-matching), so shadowing is
  correct. Workspace-wide: edits span every file referencing the symbol, grouped by URI into a
  single `WorkspaceEdit`.
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

### Circom-style include resolution

`include` paths resolve **relative to the including source file**, the way the circom compiler
resolves them — not confined to the workspace roots. Absolute include paths are refused (they
would enable an arbitrary local-file read); relative and `..` includes resolve normally and may
read files outside the workspace roots. The basename fallback is scoped to workspace-indexed
files. See [architecture.md](./architecture.md#include-resolution).

### Diagnostics

Syntax and lexer errors are reported via `textDocument/publishDiagnostics` as you type. The
error-recovering parser produces precise ranges and messages (e.g. `expect Semicolon but got
TemplateKw`); unterminated block comments and stray `*/` are surfaced as lexer errors.

## Registered but not yet implemented

Two handlers are advertised in server capabilities but currently return an empty result. See
[roadmap.md](./roadmap.md) for details:

- **Document Symbol / Outline** — `crates/lsp/src/handler/document_symbol.rs`
- **Formatting** — `crates/lsp/src/handler/formatting.rs`
