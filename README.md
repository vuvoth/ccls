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

CCLS provides **Go to Definition** (cross-file via `include`), **Hover**, **Completion**
(including `component.<signal>` member completion), semantic **Find References**, scope-aware
**Rename**, an **error-recovering parser**, lazy/cached analysis, and **sandboxed includes**
(path-traversal safe).

See [`docs/features.md`](./docs/features.md) for the full list with per-feature details, and
[`docs/roadmap.md`](./docs/roadmap.md) for what is not yet implemented.

---

## 📚 Documentation

In-depth docs live in [`docs/`](./docs/README.md):

- [Features](./docs/features.md) — what CCLS can do, and how each feature resolves symbols.
- [Architecture](./docs/architecture.md) — workspace layout, resolution core, source DB, VFS.
- [Roadmap](./docs/roadmap.md) — capabilities not yet implemented.
- [Per-crate deep dives](./docs/crates/) — `parser`, `syntax`, `lsp`, `vfs`.

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

A multi-crate Rust workspace: `parser` (lexer + event-driven parser), `syntax` (`rowan`
lossless tree + typed AST), `vfs` (in-memory virtual file system), and `lsp` (the language
server), plus the TypeScript VS Code extension in `editors/code/` and build tasks in `xtask/`.

The resolution core (`resolver.rs` + `symbol_table.rs`) is name-based and shared by
goto-definition, hover, references, and rename, over a salsa-shaped source DB that is
invalidated by draining the VFS change log.

See [`docs/architecture.md`](./docs/architecture.md) for the full design, and
[`docs/crates/`](./docs/crates/) for per-crate deep dives.

---

## 🐛 Bugs & Feature Requests

Please open an issue on the repository: https://github.com/vuvoth/ccls/issues
