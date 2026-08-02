use parser::event::Event;
use parser::grammar::entry::Scope;
use parser::lexer::{tokenize, Token};
use parser::parser::Parser;
use parser::token_kind::TokenKind;
use rowan::{GreenNodeBuilder, NodeCache};

pub use rowan::{
    api::Preorder, Direction, NodeOrToken, SyntaxText, TextRange, TextSize, TokenAtOffset,
    WalkEvent,
};

use crate::syntax_node::SyntaxNode;

/// Parse `source` as a whole circom program and build its syntax tree.
pub fn syntax_tree(source: &str) -> SyntaxNode {
    let tokens = tokenize(source);
    let events = Parser::parsing(&tokens);
    build_syntax_node(&tokens, events)
}

/// Parse `source` starting from a specific entry `scope` and build its syntax tree.
pub fn syntax_node_from_source(source: &str, scope: Scope) -> SyntaxNode {
    let tokens = tokenize(source);
    let events = Parser::parsing_with_scope(&tokens, scope);
    build_syntax_node(&tokens, events)
}

fn build_syntax_node(tokens: &[Token], events: Vec<Event>) -> SyntaxNode {
    // A fresh `NodeCache` per parse: identical tokens/subtrees are still deduplicated *within* a
    // single parse, but nothing accumulates across parses. A process-global cache would leak
    // every distinct identifier/literal ever parsed for the whole server lifetime (unbounded in a
    // long LSP session); a per-parse cache bounds memory to one parse's working set.
    let mut cache = NodeCache::default();
    let mut builder = GreenNodeBuilder::with_cache(&mut cache);
    build_green(tokens, events, &mut builder);
    let green = builder.finish();
    SyntaxNode::new_root(green)
}

/// Drive a `GreenNodeBuilder` straight from the parser's event stream, producing a tree
/// byte-identical to the previous `Output` → `build_rec` path.
///
/// Robust against malformed streams (a stray `Close`, an unclosed `Open`, or a stream with no root
/// `Open`) so that `GreenNodeBuilder::finish` — which asserts exactly one top-level node — never
/// panics, even on a grammar bug. Real parser output is always single-rooted and balanced, so these
/// guards are defense-in-depth only.
fn build_green(tokens: &[Token], events: Vec<Event>, builder: &mut GreenNodeBuilder) {
    // The first event must open the root node. An empty stream, or one whose first event is not an
    // `Open` (neither can come from the real grammar), yields an empty `ParserError` root —
    // mirroring `Output::from`'s empty-tree fallback and keeping `finish()` single-rooted.
    let mut iter = events.into_iter();
    let root_kind = match iter.next() {
        Some(Event::Open { kind }) => kind,
        _ => {
            builder.start_node(TokenKind::ParserError.into());
            builder.finish_node();
            return;
        }
    };

    // The root is held open (`open >= 1`) for the whole stream and closed by the tail loop, so a
    // stray trailing `Close` can never pop below the root.
    builder.start_node(root_kind.into());
    let mut open: u32 = 1;
    for event in iter {
        match event {
            Event::Open { kind } => {
                builder.start_node(kind.into());
                open += 1;
            }
            Event::Close => {
                // Close an inner node; a stray `Close` at the root level (`open == 1`) is dropped
                // rather than popping the root.
                if open > 1 {
                    builder.finish_node();
                    open -= 1;
                }
            }
            Event::Token(i) => {
                // The parser emits a token index only for a token it consumed, so `i` is always in
                // range; an out-of-range index (impossible for well-formed output) is dropped
                // rather than panicking. Each token is wrapped in a single-child node of the same
                // kind — this wrapping is load-bearing for `AstNode::cast`.
                if let Some(t) = tokens.get(i) {
                    builder.start_node(t.kind.into());
                    builder.token(t.kind.into(), t.text);
                    builder.finish_node();
                }
            }
            Event::ErrorReport(m) => {
                // `error_report` already opened the outer `Error` node; emit the message as a
                // nested `Error { token }` so the shape is `Error { Error { <msg> } }`, matching
                // the old `build_rec` error path exactly.
                builder.start_node(TokenKind::Error.into());
                builder.token(TokenKind::Error.into(), m.as_str());
                builder.finish_node();
            }
        }
    }

    // Close every node still open, the root last, so `finish()` always observes exactly one root.
    for _ in 0..open {
        builder.finish_node();
    }
}

#[cfg(test)]
mod test_utils;

#[cfg(test)]
mod tests {
    use parser::grammar::entry::Scope;

    use crate::syntax::test_utils::view_ast;
    use crate::test_syntax;

    #[test]
    fn pragma_happy_test() {
        test_syntax!("/src/test_files/happy/pragma.circom", Scope::Pragma);
    }

    #[test]
    fn top_level_allows_include_after_definition_test() {
        // Regression: circom permits pragma/include/template/function/main in any order. A program
        // with an `include` *after* a definition must parse the include (not treat it as a stray
        // top-level token) and produce no error node for it.
        use crate::syntax::WalkEvent;
        use parser::token_kind::TokenKind;
        use rowan::NodeOrToken;

        let src = "pragma circom 2.0.0;\ntemplate T() {}\ninclude \"lib.circom\";\n";
        let tree = crate::syntax::syntax_tree(src);

        let mut has_include = false;
        let mut has_error = false;
        for event in tree.preorder_with_tokens() {
            if let WalkEvent::Enter(NodeOrToken::Node(n)) = event {
                if n.kind() == TokenKind::Include {
                    has_include = true;
                }
                if n.kind() == TokenKind::Error || n.kind() == TokenKind::ParserError {
                    has_error = true;
                }
            }
        }
        assert!(has_include, "trailing include must be parsed");
        assert!(!has_error, "interspersed include must not produce an error");
    }

    #[test]
    fn template_happy_test() {
        // SOURCE & EXPECTED RESULT
        test_syntax!("/src/test_files/happy/template.circom", Scope::Template);
    }

    #[test]
    fn block_happy_test() {
        test_syntax!("/src/test_files/happy/block.circom", Scope::Block);
    }

    #[test]
    fn statements_happy_test() {
        test_syntax!("/src/test_files/happy/statements.circom", Scope::Block);
    }

    #[test]
    fn comment_happy_test() {
        test_syntax!(
            "/src/test_files/happy/block_comment.circom",
            Scope::CircomProgram
        );
        test_syntax!(
            "/src/test_files/happy/line_comment.circom",
            Scope::CircomProgram
        );
    }

    #[test]
    fn full_circom_program() {
        test_syntax!(
            "/src/test_files/happy/full_circom_program.circom",
            Scope::CircomProgram
        );
    }

    #[test]
    fn precedence_test() {
        // Exercises the Pratt precedence/associativity rewrite (left-assoc subtraction/division,
        // bitwise tighter than comparisons, `**` left-assoc, prefix `-` looser than `**`, ternary).
        test_syntax!("/src/test_files/happy/precedence.circom", Scope::Block);
    }

    #[test]
    fn signal_header_test() {
        // Exercises both signal-header keyword orders (`signal input` / `input signal`) and
        // optional tag lists (`{t1}`, `{t1, t2}`).
        test_syntax!("/src/test_files/happy/signal_header.circom", Scope::Block);
    }

    #[test]
    fn syntax_surface_test() {
        // Exercises constructs with no prior snapshot coverage: `include`, `component main
        // {public […]}`, hex literals, intdiv `\`, bitwise `| ^ ~ <<`, prefix `! ~`, the
        // comparison `!= >= <=`, every compound-assign family, and tuple-LHS declarations
        // (`var (a,b)=…`, `signal (a,b) <== …`) plus nested arrays `[N][M]`.
        test_syntax!(
            "/src/test_files/happy/syntax_surface.circom",
            Scope::CircomProgram
        );
    }

    #[test]
    fn error_recovery_missing_semicolon_test() {
        // Missing `;` after a pragma → `expect(Semicolon)` fails at EOF → `Event::ErrorReport` →
        // a nested `Error{ Error{ token } }` in the tree. This is the snapshot that pins the
        // `ErrorReport` branch of the builder.
        test_syntax!(
            "/src/test_files/error_recovery/missing_semicolon.circom",
            Scope::CircomProgram
        );
    }

    #[test]
    fn error_recovery_unclosed_template_test() {
        // Missing `}` for a block → `expect(RCurly)` fails at EOF → `Event::ErrorReport`, yet the
        // grammar still unconditionally closes `Block`/`TemplateDef`. Pins error recovery +
        // balanced close on malformed input.
        test_syntax!(
            "/src/test_files/error_recovery/unclosed_template.circom",
            Scope::CircomProgram
        );
    }

    #[test]
    fn error_recovery_unexpected_top_level_test() {
        // A stray token at the top level → `advance_with_error` → an `Error{ <token> }` node
        // (single wrapping, the `advance_with_error` form distinct from `error_report`).
        test_syntax!(
            "/src/test_files/error_recovery/unexpected_top_level.circom",
            Scope::CircomProgram
        );
    }
}

#[cfg(test)]
mod build_green_tests {
    //! Direct unit tests for `build_green`, replacing the intent of the deleted `output.rs` tests.
    //! They synthesize raw event/token streams and assert both correctness and that malformed input
    //! never panics rowan (empty stream, stray `Close`, unclosed `Open`).
    use super::*;
    use parser::event::Event;
    use parser::lexer::Token;
    use parser::token_kind::TokenKind;
    use rowan::NodeOrToken;

    fn tok(kind: TokenKind, text: &'static str) -> Token<'static> {
        Token {
            kind,
            text,
            range: 0..0,
        }
    }

    /// Drive `build_green` over a synthetic stream and return the resulting root node.
    fn root(tokens: &[Token<'static>], events: Vec<Event>) -> SyntaxNode {
        let mut builder = GreenNodeBuilder::new();
        build_green(tokens, events, &mut builder);
        let green = builder.finish();
        SyntaxNode::new_root(green)
    }

    #[test]
    fn well_formed_stream_builds_single_root_tree() {
        // Open(Block) Open(Expression) Token Close Close  ->  Block { Expression { token } }
        let tokens = vec![tok(TokenKind::Number, "1")];
        let events = vec![
            Event::Open {
                kind: TokenKind::Block,
            },
            Event::Open {
                kind: TokenKind::Expression,
            },
            Event::Token(0),
            Event::Close,
            Event::Close,
        ];
        let root = root(&tokens, events);
        assert_eq!(root.kind(), TokenKind::Block);
        // Exactly one top-level child: the inner Expression node.
        let children: Vec<_> = root.children().collect();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].kind(), TokenKind::Expression);
    }

    #[test]
    fn empty_stream_yields_single_parser_error_root() {
        let root = root(&[], Vec::new());
        assert_eq!(root.kind(), TokenKind::ParserError);
        assert_eq!(root.children().count(), 0);
    }

    #[test]
    fn stray_close_does_not_panic() {
        // A leading `Close` with no matching `Open` falls back to an empty `ParserError` root
        // instead of popping an empty rowan stack.
        let root = root(&[], vec![Event::Close]);
        assert_eq!(root.kind(), TokenKind::ParserError);
    }

    #[test]
    fn unclosed_open_does_not_panic() {
        // An `Open` with no matching `Close` is closed by the tail loop; `finish()` still sees a
        // single root node.
        let root = root(
            &[],
            vec![Event::Open {
                kind: TokenKind::Block,
            }],
        );
        assert_eq!(root.kind(), TokenKind::Block);
    }

    #[test]
    fn error_report_produces_nested_error_node() {
        // `error_report` emits `Open(Error) ErrorReport(msg) Close`; combined with `build_green`'s
        // own wrapping this must yield `Error { Error { <msg> } }` (double nesting).
        let events = vec![
            Event::Open {
                kind: TokenKind::Block,
            },
            Event::Open {
                kind: TokenKind::Error,
            },
            Event::ErrorReport("oops".to_string()),
            Event::Close,
        ];
        let root = root(&[], events);
        assert_eq!(root.kind(), TokenKind::Block);

        let outer: Vec<_> = root.children().collect();
        assert_eq!(outer.len(), 1);
        assert_eq!(outer[0].kind(), TokenKind::Error);

        let inner: Vec<_> = outer[0].children_with_tokens().collect();
        assert_eq!(inner.len(), 1);
        match &inner[0] {
            NodeOrToken::Node(n) => assert_eq!(n.kind(), TokenKind::Error),
            other => panic!("expected inner Error node, got {other:?}"),
        }
    }
}
