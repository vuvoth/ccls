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
            if tk == TokenKind::CommentBlockOpen {
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
            } else {
                input.kind.push(tk);
                input.position.push(lex.span());
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
    use super::Input;

    fn test(source: &str, snapshot_name: &str) {
        let input = Input::new(&source);

        insta::assert_yaml_snapshot!(snapshot_name, input);
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
}
