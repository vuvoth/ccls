use std::ops::Range;

use logos::Lexer;

use serde::Serialize;

use crate::token_kind::TokenKind;

#[derive(Debug, PartialEq, Serialize)]
pub struct Input<'a> {
    kind: Vec<TokenKind>,
    source: &'a str,
    position: Vec<Range<usize>>,
}

impl<'a> Input<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut input = Input {
            source,
            kind: Vec::new(),
            position: Vec::new(),
        };

        let mut lex = Lexer::<TokenKind>::new(source);

        while let Some(tk) = lex.next() {
            match tk {
                TokenKind::CommentBlockOpen => {
                    let mut closed = false;
                    let mut join_span = lex.span();
                    while let Some(t) = lex.next() {
                        join_span.end = lex.span().end;
                        if t == TokenKind::CommentBlockClose {
                            closed = true;
                            break;
                        }
                    }

                    if closed {
                        input.kind.push(TokenKind::BlockComment);
                    } else {
                        input.kind.push(TokenKind::Error);
                    }
                    input.position.push(join_span);
                }
                // A stray `*/` with no matching `/*` — e.g. from nested comment markers
                // (`/* a /* b */ c */`), since circom comments do NOT nest. The coalescing
                // above consumes only up to the first `*/`, so a trailing `*/` would otherwise
                // leak into the token stream as a `CommentBlockClose` the parser cannot consume.
                // Treat it as a lexing error instead.
                TokenKind::CommentBlockClose => {
                    input.kind.push(TokenKind::Error);
                    input.position.push(lex.span());
                }
                _ => {
                    input.kind.push(tk);
                    input.position.push(lex.span());
                }
            }
        }

        input
    }

    pub fn token_value(&self, index: usize) -> Option<&'a str> {
        if index < self.kind.len() {
            Some(&self.source[self.position[index].start..self.position[index].end])
        } else {
            // return None for out of bound index
            None
        }
    }

    pub fn kind_of(&self, index: usize) -> TokenKind {
        if index < self.kind.len() {
            self.kind[index]
        } else {
            TokenKind::EOF
        }
    }

    pub fn position_of(&self, index: usize) -> Option<Range<usize>> {
        if index < self.kind.len() {
            Some(self.position[index].clone())
        } else {
            // return error for out of bound index
            None
        }
    }

    pub fn size(&self) -> usize {
        self.kind.len()
    }
}

#[cfg(test)]
mod tests {
    use crate::token_kind::TokenKind;

    use super::Input;

    fn test(source: &str, snapshot_name: &str) {
        let input = Input::new(source);

        insta::assert_yaml_snapshot!(snapshot_name, input);
    }

    /// Collect just the non-trivial token kinds (drop whitespace/newline/comments) for precise
    /// assertions about the lexer fixes.
    fn kinds(source: &str) -> Vec<TokenKind> {
        let input = Input::new(source);
        input
            .kind
            .iter()
            .copied()
            .filter(|k| !k.is_trivial())
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
        assert_eq!(Input::new("2.0").kind, vec![TokenKind::Error]);
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
        assert_eq!(Input::new("1.5").kind, vec![TokenKind::Error]);
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
        let raw = Input::new("/* a /* b */ c */");
        assert!(
            !raw.kind.contains(&TokenKind::CommentBlockClose),
            "stray `*/` leaked into the token stream"
        );
    }
}
