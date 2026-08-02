//! Typed token stream: the lexer→parser boundary.
//!
//! [`tokenize`] produces a flat `Vec<Token>` including trivia (whitespace, newlines, comments),
//! matching the lossless-tree requirements of the rowan syntax tree. Block comments are coalesced
//! here into a single [`TokenKind::BlockComment`] (or a single [`TokenKind::Error`] when
//! unterminated / stray), so the internal `CommentBlockOpen`/`CommentBlockClose` logos variants
//! never reach consumers.

use std::ops::Range;

use logos::Lexer;
use serde::Serialize;

use crate::token_kind::TokenKind;

/// A single lexed token: its kind, the source text it covers, and its byte range within `source`.
///
/// All three fields describe the same span — `text` is always `&source[range]` — but carrying
/// kind, text, and range together lets the parser and the syntax builder consume tokens by value
/// instead of re-resolving integer indices back into parallel arrays.
///
/// Not `Copy`: `Range<usize>` is not `Copy` (it is `Clone`), and every consumer borrows tokens
/// (`&[Token]`, `&Token`) rather than moving them, so `Clone` is sufficient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub text: &'a str,
    pub range: Range<usize>,
}

/// Tokenize `source` into a flat token sequence including trivia.
///
/// The token stream is byte-identical to the legacy `Input::new` output: the same logos rules
/// produce the same kinds in the same order, block comments are coalesced with identical span
/// joining, and a stray `*/` (with no matching `/*`) becomes an `Error`.
pub fn tokenize<'a>(source: &'a str) -> Vec<Token<'a>> {
    let mut tokens: Vec<Token<'a>> = Vec::new();
    let mut lex = Lexer::<TokenKind>::new(source);

    while let Some(kind) = lex.next() {
        let span = lex.span();
        match kind {
            TokenKind::CommentBlockOpen => {
                // Coalesce a block comment into one `BlockComment` token. circom comments do NOT
                // nest, so consume up to (and including) the first `CommentBlockClose`; the joined
                // span runs from the `/*` start to the `*/` end.
                let mut closed = false;
                let mut join_span = span;
                while let Some(t) = lex.next() {
                    join_span.end = lex.span().end;
                    if t == TokenKind::CommentBlockClose {
                        closed = true;
                        break;
                    }
                }

                let coalesced = if closed {
                    TokenKind::BlockComment
                } else {
                    TokenKind::Error
                };
                tokens.push(token(source, coalesced, join_span));
            }
            // A stray `*/` with no matching `/*` — e.g. from nested comment markers
            // (`/* a /* b */ c */`), since circom comments do NOT nest. The coalescing above
            // consumes only up to the first `*/`, so a trailing `*/` would otherwise leak into the
            // token stream as a `CommentBlockClose` the parser cannot consume. Treat it as a
            // lexing error instead.
            TokenKind::CommentBlockClose => {
                tokens.push(token(source, TokenKind::Error, span));
            }
            _ => {
                tokens.push(token(source, kind, span));
            }
        }
    }

    tokens
}

/// Build a `Token` from a logos-derived span.
///
/// The span always originates from `logos` (which guarantees char-aligned bounds within `source`)
/// or from joining two such spans end-to-end (`CommentBlockOpen` start + `CommentBlockClose` end),
/// so indexing `source[span]` is always in-bounds and on a char boundary.
fn token<'a>(source: &'a str, kind: TokenKind, span: Range<usize>) -> Token<'a> {
    let text = &source[span.start..span.end];
    Token {
        kind,
        text,
        range: span,
    }
}

#[cfg(test)]
mod tests {
    use crate::token_kind::TokenKind;

    use super::tokenize;

    fn test(source: &str, snapshot_name: &str) {
        let tokens = tokenize(source);
        insta::assert_yaml_snapshot!(snapshot_name, &tokens);
    }

    /// Non-trivial token kinds (whitespace/newline/comments/errors dropped) — for precise
    /// assertions about the lexer fixes that mirror what the parser actually consumes.
    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .iter()
            .map(|t| t.kind)
            .filter(|k| !k.is_trivial())
            .collect::<Vec<TokenKind>>()
    }

    /// Every token kind in stream order, trivia included — for assertions about the raw stream
    /// (e.g. that a single `Error` is emitted, or that no internal scaffolding leaks).
    fn kinds_all(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .iter()
            .map(|t| t.kind)
            .collect::<Vec<TokenKind>>()
    }

    #[test]
    fn test_comment_block() {
        let source = r#"
        /*a + b == 10*/
        a + 10
    "#;
        test(source, "test_comment_block");
    }

    #[test]
    fn test_comment_error() {
        let source = r#"
        pragma 2.1.1;
        /*a + b == 10*
        a + 10
        template

        /*
    "#;
        test(source, "test_comment_error");
    }

    #[test]
    fn test_pragma() {
        let source = r#"
        /* test pragma token kinds */

    pragma circom 2.0.0;

    "#;
        test(source, "test_pragma");
    }

    #[test]
    fn test_function() {
        let source = r#"
    function nbits(a) {
        var n = 1;
        var r = 0;
        while (n-1<a) {
            r++;
            n *= 2;
        }
        return r;
    }"#;
        test(source, "test_function");
    }

    #[test]
    fn test_operators() {
        let source = r#"
        ({[]})
        ;.,:
        && &
        || |
        != !
        === == =
        --> ==>
        <-- <==
        <= <
        >= >
        ++ += +
        -- -= -
        **= **
        * *=
        / /=
        \ \=
        % %=
        ^ ^=
        ~ ~=
        >> >>=
        << <<=
        & &=
        | |=
    }"#;
        test(source, "test_operators");
    }

    #[test]
    fn version_lexes_three_parts() {
        // Grammar terminal is N.N.N. Multi-digit components must lex as a single Version token
        // (the old regex `2.[0-9].[0-9]` only accepted single digits and matched any char for `.`).
        assert_eq!(kinds("2.0.0"), vec![TokenKind::Version]);
        assert_eq!(kinds("2.1.12"), vec![TokenKind::Version]);
        assert_eq!(kinds("22.0.0"), vec![TokenKind::Version]);
        assert_eq!(
            kinds("pragma circom 2.1.12;"),
            vec![
                TokenKind::PragmaKw,
                TokenKind::Circom,
                TokenKind::Version,
                TokenKind::Semicolon,
            ]
        );
        assert_eq!(kinds("2"), vec![TokenKind::Number]);
        // Note: circom has no floats, so `<num>.<num>` (fewer than 3 dotted parts) is invalid.
        // logos commits to the Version prefix `[0-9]+\.[0-9]+\.` and, when it fails to complete,
        // emits a single Error for the span instead of falling back to Number+Dot+Number.
        // This is acceptable (the input is invalid circom) and was already the behavior of the
        // previous `2.[0-9].[0-9]` regex; we assert it here to pin the behavior.
        assert_eq!(kinds_all("2.0"), vec![TokenKind::Error]);
    }

    #[test]
    fn hex_number_lexes_as_one_token() {
        // Previously `0x1F` mis-lexed as Number `0` + Identifier `x1F`.
        assert_eq!(kinds("0x1F"), vec![TokenKind::HexNumber]);
        assert_eq!(kinds("0xDEADBEEF"), vec![TokenKind::HexNumber]);
        assert_eq!(kinds("0xabcdef"), vec![TokenKind::HexNumber]);
        assert_eq!(kinds("255"), vec![TokenKind::Number]);
        assert_eq!(kinds("0"), vec![TokenKind::Number]);
    }

    #[test]
    fn number_identifier_boundaries() {
        // circom has no floats: `1.5` is invalid and lexes as a single Error (see
        // version_lexes_three_parts for the logos prefix-commit explanation).
        assert_eq!(kinds_all("1.5"), vec![TokenKind::Error]);
        // `123abc` -> Number then Identifier (number cannot include a letter).
        assert_eq!(
            kinds("123abc"),
            vec![TokenKind::Number, TokenKind::Identifier]
        );
        assert_eq!(kinds("_foo"), vec![TokenKind::Identifier]);
        assert_eq!(kinds("$bar"), vec![TokenKind::Identifier]);
    }

    #[test]
    fn nested_block_comment_does_not_leak_stray_close() {
        // circom comments do not nest, so `/* a /* b */ c */` closes at the first `*/`.
        // The trailing `*/` must NOT leak as a `CommentBlockClose` token (the bug this fixes);
        // it becomes an Error (which is trivial and skipped by the parser).
        let tokens = kinds("/* a /* b */ c */");
        // Only the non-trivial `c` identifier remains; the stray `*/` is a trivial Error.
        assert_eq!(tokens, vec![TokenKind::Identifier]);
        // And the raw stream contains no CommentBlockClose token at all.
        let raw = kinds_all("/* a /* b */ c */");
        assert!(
            !raw.contains(&TokenKind::CommentBlockClose),
            "stray `*/` leaked into the token stream"
        );
    }
}
