use parser::event::Event;
use parser::grammar::entry::Scope;
use parser::lexer::{tokenize, tokenize_with_errors, Token};
use parser::parser::Parser;
use parser::token_kind::TokenKind;
use rowan::{GreenNodeBuilder, NodeCache};

pub use rowan::{
    api::Preorder, Direction, NodeOrToken, SyntaxText, TextRange, TextSize, TokenAtOffset,
    WalkEvent,
};

use crate::node::SyntaxNode;

/// A syntax error: a source range plus a message. Collected during the build from the parser's
/// `ErrorReport` events (and the lexer's errors) and surfaced as LSP diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub range: rowan::TextRange,
    pub msg: String,
}

/// The product of a parse: the lossless syntax tree plus the errors found in it.
#[derive(Debug, Clone)]
pub struct Parse {
    pub tree: SyntaxNode,
    pub errors: Vec<SyntaxError>,
}

/// Parse `source` as a whole circom program, returning the tree and any errors.
pub fn parse(source: &str) -> Parse {
    let (tokens, lex_errors) = tokenize_with_errors(source);
    let events = Parser::parse(&tokens);
    let (tree, parse_errors) = build_syntax_node(&tokens, events);
    let mut errors: Vec<SyntaxError> = lex_errors
        .into_iter()
        .map(|le| SyntaxError {
            range: text_range(&le.range),
            msg: le.msg,
        })
        .collect();
    errors.extend(parse_errors);
    Parse { tree, errors }
}

/// Parse `source` as a whole circom program and build its syntax tree (errors discarded).
pub fn syntax_tree(source: &str) -> SyntaxNode {
    parse(source).tree
}

/// Parse `source` starting from a specific entry `scope` and build its syntax tree.
pub fn syntax_node_from_source(source: &str, scope: Scope) -> SyntaxNode {
    let tokens = tokenize(source);
    let events = Parser::parse_with_scope(&tokens, scope);
    build_syntax_node(&tokens, events).0
}

/// Convert a lexer byte range into a `rowan::TextRange`.
fn text_range(r: &std::ops::Range<usize>) -> rowan::TextRange {
    rowan::TextRange::new(
        rowan::TextSize::from(r.start as u32),
        rowan::TextSize::from(r.end as u32),
    )
}

fn build_syntax_node(tokens: &[Token], events: Vec<Event>) -> (SyntaxNode, Vec<SyntaxError>) {
    let mut cache = NodeCache::default();
    let mut builder = GreenNodeBuilder::with_cache(&mut cache);
    let errors = build_green(tokens, events, &mut builder);
    let green = builder.finish();
    (SyntaxNode::new_root(green), errors)
}

/// Per-open-node state tracked during the build. `Err` frames record the first token they wrap
/// (if any) and any `ErrorReport` message, so on close they can be turned into a [`SyntaxError`].
enum Frame {
    Normal,
    Err {
        token: Option<rowan::TextRange>,
        msg: Option<String>,
    },
}

/// Drive a `GreenNodeBuilder` from the parser's event stream, returning the syntax errors.
///
/// Robust against malformed streams so `GreenNodeBuilder::finish` (which asserts exactly one
/// top-level node) never panics.
fn build_green(
    tokens: &[Token],
    events: Vec<Event>,
    builder: &mut GreenNodeBuilder,
) -> Vec<SyntaxError> {
    let mut errors: Vec<SyntaxError> = Vec::new();
    let mut next_idx: usize = 0;
    let mut stack: Vec<Frame> = Vec::new();

    let mut iter = events.into_iter();
    let root_kind = match iter.next() {
        Some(Event::Open { kind }) => kind,
        _ => {
            builder.start_node(TokenKind::ParserError.into());
            builder.finish_node();
            return errors;
        }
    };

    builder.start_node(root_kind.into());
    stack.push(Frame::Normal);

    let close_frame = |frame: Frame, errors: &mut Vec<SyntaxError>, next_idx: usize| {
        if let Frame::Err { token, msg } = frame {
            let range = match (token, msg.is_some()) {
                (Some(r), _) => r,
                (None, true) => match tokens.get(next_idx) {
                    Some(t) => text_range(&t.range),
                    None => {
                        let end = tokens.last().map(|t| t.range.end).unwrap_or(0);
                        TextRange::new(TextSize::from(end as u32), TextSize::from(end as u32))
                    }
                },
                (None, false) => return,
            };
            let message = msg.unwrap_or_else(|| "unexpected token".to_string());
            errors.push(SyntaxError {
                range,
                msg: message,
            });
        }
    };

    for event in iter {
        match event {
            Event::Open { kind } => {
                builder.start_node(kind.into());
                stack.push(if kind == TokenKind::Error {
                    Frame::Err {
                        token: None,
                        msg: None,
                    }
                } else {
                    Frame::Normal
                });
            }
            Event::Close => {
                if stack.len() > 1 {
                    let frame = stack.pop().unwrap();
                    builder.finish_node();
                    close_frame(frame, &mut errors, next_idx);
                }
            }
            Event::Token(i) => {
                if let Some(t) = tokens.get(i) {
                    builder.start_node(t.kind.into());
                    builder.token(t.kind.into(), t.text);
                    builder.finish_node();
                    if let Some(Frame::Err { token, .. }) = stack.last_mut() {
                        if token.is_none() {
                            *token = Some(text_range(&t.range));
                        }
                    }
                    if i + 1 > next_idx {
                        next_idx = i + 1;
                    }
                }
            }
            Event::ErrorReport(msg) => {
                builder.start_node(TokenKind::Error.into());
                builder.finish_node();
                if let Some(Frame::Err { msg: slot, .. }) = stack.last_mut() {
                    *slot = Some(msg);
                }
            }
        }
    }

    while stack.len() > 1 {
        let frame = stack.pop().unwrap();
        builder.finish_node();
        close_frame(frame, &mut errors, next_idx);
    }
    builder.finish_node();

    errors
}

#[cfg(test)]
mod test_utils;

#[cfg(test)]
mod tests {
    use parser::grammar::entry::Scope;

    use crate::test_syntax;
    use crate::tree::test_utils::view_ast;

    #[test]
    fn pragma_happy_test() {
        test_syntax!("/src/test_files/happy/pragma.circom", Scope::Pragma);
    }

    #[test]
    fn top_level_allows_include_after_definition_test() {
        // Regression: circom permits pragma/include/template/function/main in any order. A program
        // with an `include` *after* a definition must parse the include (not treat it as a stray
        // top-level token) and produce no error node for it.
        use crate::tree::WalkEvent;
        use parser::token_kind::TokenKind;
        use rowan::NodeOrToken;

        let src = "pragma circom 2.0.0;\ntemplate T() {}\ninclude \"lib.circom\";\n";
        let tree = crate::tree::syntax_tree(src);

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

#[cfg(test)]
mod parse_error_tests {
    use super::parse;

    #[test]
    fn clean_program_has_no_errors() {
        let src = "pragma circom 2.0.0;\ntemplate T() { signal output o; o <== 0; }\n";
        let parsed = parse(src);
        assert!(
            parsed.errors.is_empty(),
            "unexpected errors: {:?}",
            parsed.errors
        );
    }

    #[test]
    fn missing_semicolon_yields_error() {
        // `expect(Semicolon)` fails at EOF → one ErrorReport with a range at end of input.
        let parsed = parse("pragma circom 2.0.0");
        assert_eq!(parsed.errors.len(), 1, "{:?}", parsed.errors);
        assert!(
            parsed.errors[0].msg.contains("Semicolon"),
            "{:?}",
            parsed.errors[0]
        );
    }

    #[test]
    fn unclosed_template_yields_error() {
        let parsed = parse("template T() { signal output o; o <== 0;");
        assert!(
            !parsed.errors.is_empty(),
            "unclosed block should report an error"
        );
    }

    #[test]
    fn unterminated_block_comment_is_a_lexer_error() {
        let parsed = parse("/* never closed");
        assert!(
            parsed
                .errors
                .iter()
                .any(|e| e.msg.contains("block comment")),
            "{:?}",
            parsed.errors
        );
    }

    #[test]
    fn stray_close_comment_is_a_lexer_error() {
        let parsed = parse("a */ b");
        assert!(
            parsed.errors.iter().any(|e| e.msg.contains("*/")),
            "{:?}",
            parsed.errors
        );
    }

    #[test]
    fn error_ranges_are_within_source() {
        let src = "pragma circom 2.0.0";
        let parsed = parse(src);
        let end = rowan::TextSize::from(src.len() as u32);
        for e in &parsed.errors {
            assert!(
                e.range.start() <= end && e.range.end() <= end,
                "range out of bounds: {:?}",
                e
            );
        }
    }
}
