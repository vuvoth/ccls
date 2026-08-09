# Roadmap

Features not yet implemented in CCLS. Implemented features are listed in
[features.md](./features.md).

## Placeholders (capability registered, returns empty)

These handlers exist but currently return `None`:

- [ ] **Document Symbol / Outline** — `textDocument/documentSymbol`. Walk the program AST and
      emit one symbol per template / function / signal / variable / component with its
      location and kind. See `crates/lsp/src/handler/document_symbol.rs`.
- [ ] **Formatting** — `textDocument/formatting`. Re-tokenize the document and normalize
      whitespace/indentation into `TextEdit`s. See
      `crates/lsp/src/handler/formatting.rs`.

## Not yet implemented

- [ ] **Semantic Highlighting** — `textDocument/semanticTokens`.
- [ ] **Signature Help** — `textDocument/signatureHelp`.
- [ ] **Code Actions / Quick Fixes** — `textDocument/codeAction`.
- [ ] **Folding Ranges** — `textDocument/foldingRange`.
- [ ] **Document Highlight** — `textDocument/documentHighlight`.
- [ ] **Selection Range** — `textDocument/selectionRange`.
- [ ] **Inlay Hints** — `textDocument/inlayHint`.
- [ ] **Member-field references/rename** — `c.out` resolves for goto-def/hover, but Find
      References and Rename of a component signal field are a no-op (they ride the flat
      resolver). Needs per-template occurrence search.

## Existing features — follow-ups

- [ ] **Doc-comment parsing** — richer hover derived from circom comments.
- [ ] **Incremental sync** — document sync is currently `Full`; switch to incremental
      `didChange` ranges.
