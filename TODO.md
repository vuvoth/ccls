# Roadmap

Features not yet implemented in CCLS. Implemented features are listed in the
[README](./README.md#-features).

## Placeholders (capability registered, returns empty)

These handlers exist but currently return `None`:

- [ ] **Document Symbol / Outline** — `textDocument/documentSymbol`. Walk the program AST and emit one
      symbol per template / function / signal / variable / component with its location and kind.
      See `crates/lsp/src/handler/document_symbol.rs`.
- [ ] **Formatting** — `textDocument/formatting`. Re-tokenize the document and normalize
      whitespace/indentation into `TextEdit`s. See `crates/lsp/src/handler/formatting.rs`.

## Not yet implemented

- [ ] **Diagnostics** — syntax/semantic error reporting (`textDocument/publishDiagnostics`). The
      parser already produces errors via `error_report()`; surface them to the client.
- [ ] **Semantic Highlighting** — `textDocument/semanticTokens`.
- [ ] **Signature Help** — `textDocument/signatureHelp`.
- [ ] **Code Actions / Quick Fixes** — `textDocument/codeAction`.
- [ ] **Folding Ranges** — `textDocument/foldingRange`.
- [ ] **Document Highlight** — `textDocument/documentHighlight`.
- [ ] **Selection Range** — `textDocument/selectionRange`.
- [ ] **Inlay Hints** — `textDocument/inlayHint`.

## Existing features — follow-ups

- [ ] **Cross-file Rename & References** — both are currently in-file only. A workspace-wide symbol
      graph is needed instead of name/`def_range` matching across files (which both misses real
      cross-file usages and can collide when two files define a same-named symbol at the same
      line:column).
- [ ] **Doc-comment parsing** — richer hover derived from circom comments.
- [ ] **Incremental sync** — document sync is currently `Full`; switch to incremental `didChange`
      ranges.
