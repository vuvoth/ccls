# CCLS Documentation

This folder holds the in-depth documentation for **CCLS**, the Circom language server.

For the quick-start (clone, install Rust, build, run tests), see the root
[`README.md`](../README.md). For the VS Code extension package, see
[`editors/code/README.md`](../editors/code/README.md).

## Contents

| Document | Description |
| --- | --- |
| [Features](./features.md) | What CCLS can do today, and how each feature resolves symbols. |
| [Architecture](./architecture.md) | Workspace layout, the resolution core, the source DB, and the VFS. |
| [Roadmap](./roadmap.md) | Capabilities not yet implemented and follow-ups on existing features. |

### Per-crate deep dives

| Crate | Document | Responsibility |
| --- | --- | --- |
| `parser` | [crates/parser.md](./crates/parser.md) | `logos` lexer + event-driven parser with markers and grammar modules. |
| `syntax` | [crates/syntax.md](./crates/syntax.md) | `rowan` lossless syntax tree and typed AST. |
| `lsp` | [crates/lsp.md](./crates/lsp.md) | LSP server: handlers, global state, resolver, source DB, symbol table. |
| `vfs` | [crates/vfs.md](./crates/vfs.md) | In-memory, path-interned virtual file system with a change log. |
