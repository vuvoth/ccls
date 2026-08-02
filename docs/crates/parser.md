# `parser` crate

The Circom **lexer**, an **event-driven parser**, and the **grammar** rules. This crate turns
raw `.circom` source text into a flat sequence of parser events that the
[`syntax`](./syntax.md) crate builds into a lossless syntax tree.

The grammar rules follow the official circom compiler grammar
(`iden3/circom` → `parser/src/lang.lalrpop`).

## Layout

```
crates/parser/src/
├── lib.rs          # crate root: re-exports the modules below
├── lexer.rs        # logos-based tokenizer -> Vec<Token>
├── token_kind.rs   # TokenKind enum: infix()/prefix()/postfix(), is_trivial(), keywords
├── parser.rs       # the Parser struct + markers, fuel, depth guard, trivia handling
├── event.rs        # Event { Open, Close, Token } emitted by the parser
└── grammar/        # one module per language construct
    ├── block.rs
    ├── bus.rs
    ├── declaration.rs
    ├── definition.rs
    ├── expression.rs
    ├── function.rs
    ├── include.rs
    ├── list.rs
    ├── main_component.rs
    ├── pragma.rs
    ├── statement.rs
    └── template.rs
```

## Lexing (`lexer.rs`)

Uses [`logos`](https://docs.rs/logos) to scan source text into a `Vec<Token<'a>>`, where each
`Token` carries its `TokenKind` and a text slice. Whitespace and comments are kept as tokens
(trivia) rather than dropped, so the downstream tree stays **lossless**.

## Token kinds (`token_kind.rs`)

`TokenKind` is the source of truth for operator parsing. Its associated functions drive
Pratt / precedence-climbing in the expression grammar:

- `infix()` — binary operator bindings (precedence + associativity).
- `prefix()` — unary prefix operators.
- `postfix()` — unary postfix operators.
- `is_trivial()` — whitespace/comments, handled by `wrap_trivia()`.

Reserved keywords are also enumerated here; `crates/lsp/src/handler/completion.rs` mirrors
them for keyword completion.

## Event-driven parser (`parser.rs` + `event.rs`)

The parser does **not** build a tree directly. It emits a flat `Vec<Event>` of:

- `Event::Open` / `Event::Close` — node boundaries, identified by a `Marker`.
- `Event::Token` — a leaf token (by position into the token slice).

Markers (`Marker::Open`, `Marker::Close`) let grammar code open a node, parse its children,
and close it with a specific kind — the same model rust-analyzer uses. This indirection is
what enables **error recovery**: a partially-parsed node can be closed early without aborting
the whole parse.

### Guards

- **`fuel`** — a budget (reset to 256 whenever the parser advances on non-trivia). If the
  parser stops advancing, fuel runs out and recovery kicks in; this catches non-advancing
  infinite loops.
- **`depth`** — the current expression-nesting depth, incremented in the Pratt recursive
  core. Bounds recursion on deeply-nested untrusted input so a pathological file cannot
  overflow the stack (fuel only catches non-advancing loops, not deep ones).

### Trivia

`wrap_trivia()` emits leading whitespace/comments at/after the current position and returns
the next non-trivial token kind. `current()` wraps it. `peek()` does lookahead **without**
emitting trivia, so it is safe to call for dispatch decisions before/after `current()` without
double-emitting.

## Grammar (`grammar/`)

Each language construct has its own module. Expressions use Pratt / precedence climbing via
the `TokenKind` operator methods. Error recovery is supported through the
`error_report()` mechanism, which records parse errors into the event stream while continuing
to parse.

## Tests

Lexer and grammar tests use `insta` snapshots under `crates/parser/src/snapshots/`. Run them
with `cargo test -p parser`.
