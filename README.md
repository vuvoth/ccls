# CCLS

A language server for [Circom](https://docs.circom.io/), built with Rust and TypeScript.

CCLS provides rich editor support for [Circom](https://docs.circom.io/) — the DSL for writing
zero-knowledge proof circuits — with an **error-recovering parser** that stays useful even on
partially-typed or invalid files.

The project is split across:

- **Rust backend** — a multi-crate workspace: `parser` (lexer + event-driven parser),
  `syntax` (lossless `rowan` AST), `vfs` (virtual file system), and `lsp` (the language server).
- **VS Code extension** — TypeScript client (`circom-plus`) published to the marketplace.

## ✨ Features

### Implemented

- [x] **Go to Definition** — resolves signals, variables, parameters, templates, functions, and
      components, including **cross-file** jumps through `include` statements, and jumping straight
      into an included library file from its `"path.circom"` string.
- [x] **Hover** — shows the symbol kind and its declaration signature (header only for block-bodied
      defs like `template`/`function`/`bus`).
- [x] **Completion** — in-scope body symbols, file top-level names, reserved keywords, and **member
      completion** (`component.<signal>`) that resolves a component's template across files.
- [x] **Find References** — every occurrence of a symbol, resolved *semantically* (not text-matched),
      so shadowing is respected.
- [x] **Rename** — scope-aware rename with `prepareRename` support; refuses keywords, include-path
      strings, illegal names, and unresolved member-access fields.
- [x] **Error-recovering parser** — keeps working on invalid/partial circom files.
- [x] **Lazy, cached analysis** — parsing and symbol tables are memoized and invalidated only on real
      edits; includes are read from disk once.
- [x] **Sandboxed includes** — `include` resolution is confined to workspace roots
      (path-traversal / symlink-safe).

> See [`TODO.md`](./TODO.md) for the roadmap of features not yet implemented.

---

## 🚀 Installation

1. **Clone the repository:**
    ```bash
    git clone https://github.com/vuvoth/ccls.git
    cd ccls
    ```

2. **Install Rust** (if not already installed):  
   👉 https://www.rust-lang.org/tools/install

3. **Build or test the project:**
    ```bash
    cargo test     # Run tests
    cargo build    # Build the project
    ```

---

## 🧪 Running Tests (with `insta` snapshots)

Optional, but recommended for snapshot testing.

1. **Install `cargo-insta`:**
    ```bash
    curl -LsSf https://insta.rs/install.sh | sh
    ```

2. **Run the tests:**
    ```bash
    cargo test
    ```

3. **Review snapshot changes:**
    ```bash
    cargo insta review
    ```

📘 More info: [Insta Quickstart](https://insta.rs/docs/quickstart/)

---

## 🐞 Debugging the Extension

1. **Install CCLS server and client:**
    ```bash
    cargo xtask install --server
    cargo xtask install --client
    npm audit fix --force   # optional
    ```

2. **Run the extension in VSCode:**
    - Open the `ccls` project in VSCode.
    - Open the *Run and Debug* panel.
    - Select `Run Extension (Debug Build)` and start debugging.

3. A new VSCode window will open.  
   Open a Circom file and try features like **Go to Definition**.

---

## 🏗️ Architecture

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

Key design notes:

- **Resolution core** (`resolver.rs` + `semantic.rs`) is name-based and shared by goto-definition,
  hover, references, and rename — so the same symbol resolves consistently across features.
- **Source database** (`source_db.rs`) memoizes parse / file DB / symbol table per file, drained from
  the VFS change log so editing file A never recomputes file B.
- Includes are loaded once from disk, cached, and confined to workspace roots.

---

## 🐛 Bugs & Feature Requests

Please open an issue on the repository: https://github.com/vuvoth/ccls/issues
