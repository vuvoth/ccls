# `syntax` crate

Builds a **lossless**, typed syntax tree over circom source using
[`rowan`](https://docs.rs/rowan). It consumes the flat event stream produced by the
[`parser`](./parser.md) crate and exposes both a generic `SyntaxNode` tree and a typed AST.

"Lossless" means whitespace, comments, and even parse errors are preserved in the tree, so it
round-trips to the original source exactly.

## Layout

```
crates/syntax/src/
├── lib.rs                      # crate root
├── node.rs                     # CircomLanguage, SyntaxNode/Token aliases
├── tree.rs                     # syntax_tree() / build_green(): events -> rowan green tree
├── tree/test_utils.rs          # view_ast() snapshot helper
├── abstract_syntax_tree/       # typed AST nodes (AstNode impls)
│   ├── mod.rs
│   ├── program.rs
│   ├── definition.rs
│   ├── declaration.rs
│   ├── statement.rs
│   ├── expression.rs
│   ├── block.rs
│   └── name.rs
├── test_files/                 # circom fixtures (happy/ + error_recovery/)
└── snapshots/                  # committed insta snapshots
```

## The language type (`node.rs`)

`rowan` is parameterized by a `Language`. `CircomLanguage` binds rowan's `SyntaxKind` to the
parser's `TokenKind` (a `#[repr(u16)]` enum), with a compile-time
`size_of::<TokenKind>() == 2` assertion guaranteeing the `transmute` in `kind_from_raw` is
sound. The module then defines the conventional aliases: `SyntaxNode`, `SyntaxToken`,
`SyntaxElement`, `SyntaxNodeChildren`, `PreorderWithTokens`.

## From events to a green tree (`tree.rs`)

- `syntax_tree(source)` tokenizes, parses as a whole circom program, and builds the tree.
- `syntax_node_from_source(source, scope)` parses starting from a specific entry `Scope`
  (used by targeted grammar tests).
- `build_green(tokens, events, builder)` drives a `rowan::GreenNodeBuilder` straight from the
  parser's event stream (`Open` / `Close` / `Token` / `ErrorReport`).

Design points worth knowing:

- **Fresh `NodeCache` per parse** — identical tokens/subtrees are deduplicated *within* one
  parse, but nothing accumulates across parses. A process-global cache would leak every
  distinct identifier ever parsed for the whole server lifetime (unbounded in a long LSP
  session); a per-parse cache bounds memory to one parse's working set.
- **Defense-in-depth against malformed streams** — `build_green` is robust against a stray
  `Close`, an unclosed `Open`, or a stream with no root `Open`, so `GreenNodeBuilder::finish`
  (which asserts exactly one top-level node) never panics even on a grammar bug. An empty
  stream yields an empty `ParserError` root.
- **Error nodes are zero-width** — `Event::ErrorReport(msg)` becomes a childless `Error`
  node. The message is diagnostic metadata, **not** source text: emitting it as a token would
  make rowan size the node by the message length and break offset/range math. `has_error` /
  `parses_clean` key off the node kind, not text.
- **Tokens are wrapped** — each consumed token becomes a single-child node of the same kind;
  this wrapping is load-bearing for `AstNode::cast`.

## Typed AST (`abstract_syntax_tree/`)

The generic `SyntaxNode` tree is lossless but awkward to walk. The AST modules add typed
wrappers (`AstCircomProgram`, `AstTemplateDef`, `AstSignalDecl`, `AstComponentDecl`,
`AstIdentifier`, etc.) that implement rowan's `AstNode` trait, so consumers (the LSP
`resolver` and `symbol_table`) navigate the tree ergonomically via `cast` and accessor
methods rather than raw `kind()` checks.

## Tests and snapshots

- Fixtures live in `test_files/happy/` (valid) and `test_files/error_recovery/` (malformed).
- The `test_syntax!(path, scope)` macro parses a fixture and snapshots both the tree and an
  AST view via `view_ast()`.
- Snapshots are committed under `snapshots/`.
- Error-recovery tests pin behavior on missing `;`, unclosed `{`, and stray top-level tokens.
- `build_green_tests` synthesize raw event streams to assert `build_green` never panics on
  malformed input.

Run them with `cargo test -p syntax`.
