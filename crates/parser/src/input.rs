use std::ops::Range;

use logos::Lexer;

use crate::token_kind::TokenKind;

#[derive(Debug, PartialEq)]
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
    use crate::token_kind::TokenKind::{self, *};

    use super::Input;

    fn test(source: &str, expected_input: Input) {
        let input = Input::new(&source);

        assert_eq!(
            expected_input, input,
            "Tokens extract from source code are not correct"
        );

        // test size method
        let expected_size = input.kind.len();
        let size = input.size();
        assert_eq!(expected_size, size, "size method failed");

        // test methods with index out of bound
        let index = input.kind.len();

        let expected_token_value = None;
        let token_value = input.token_value(index);
        assert_eq!(
            expected_token_value, token_value,
            "token_value failed (case: index out of bound)"
        );

        let expected_kind = TokenKind::EOF;
        let kind = input.kind_of(index);
        assert_eq!(
            expected_kind, kind,
            "kind_of failed (case: index out of bound)"
        );

        let expected_position = None;
        let position = input.position_of(index);
        assert_eq!(
            expected_position, position,
            "position_of failed (case: index out of bound)"
        );

        // test methods with index in bound
        if input.size() == 0 {
            return;
        }

        let index = input.size() / 2; // a valid index if input size > 0

        let expected_token_value = &input.source[input.position[index].clone()];
        let token_value = input.token_value(index).unwrap();
        assert_eq!(expected_token_value, token_value, "token_value failed");

        let expected_kind = input.kind[index];
        let kind = input.kind_of(index);
        assert_eq!(expected_kind, kind, "kind_of failed");

        let expected_position = input.position[index].clone();
        let position = input.position_of(index).unwrap();
        assert_eq!(expected_position, position, "position_of failed");
    }

    #[test]
    fn test_comment_block() {
        let source = r#"
        /*a + b == 10*/
        a + 10
    "#;

        let expected_input = Input {
            kind: vec![
                TokenKind::EndLine,
                TokenKind::WhiteSpace,
                TokenKind::BlockComment,
                TokenKind::EndLine,
                TokenKind::WhiteSpace,
                TokenKind::Identifier,
                TokenKind::WhiteSpace,
                TokenKind::Add,
                TokenKind::WhiteSpace,
                TokenKind::Number,
                TokenKind::EndLine,
                TokenKind::WhiteSpace,
            ],
            source: &source,
            position: vec![
                { 0..1 },
                { 1..9 },
                { 9..24 },
                { 24..25 },
                { 25..33 },
                { 33..34 },
                { 34..35 },
                { 35..36 },
                { 36..37 },
                { 37..39 },
                { 39..40 },
                { 40..44 },
            ],
        };

        test(source, expected_input);
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

        let expected_input = Input {
            kind: vec![
                TokenKind::EndLine,
                TokenKind::WhiteSpace,
                TokenKind::Pragma,
                TokenKind::WhiteSpace,
                TokenKind::Version,
                TokenKind::Semicolon,
                TokenKind::EndLine,
                TokenKind::WhiteSpace,
                TokenKind::Error,
            ],
            source: &source,
            position: vec![
                0..1,
                1..9,
                9..15,
                15..16,
                16..21,
                21..22,
                22..23,
                23..31,
                31..94,
            ],
        };

        test(source, expected_input);
    }

    #[test]
    fn test_pragma() {
        let source = r#"
        /* test pragma token kinds */

    pragma circom 2.0.0;

    "#;

        let expected_input = Input {
            kind: vec![
                EndLine,
                WhiteSpace,
                BlockComment,
                EndLine,
                EndLine,
                WhiteSpace,
                Pragma,
                WhiteSpace,
                Circom,
                WhiteSpace,
                Version,
                Semicolon,
                EndLine,
                EndLine,
                WhiteSpace,
            ],
            source: &source,
            position: vec![
                0..1,
                1..9,
                9..38,
                38..39,
                39..40,
                40..44,
                44..50,
                50..51,
                51..57,
                57..58,
                58..63,
                63..64,
                64..65,
                65..66,
                66..70,
            ],
        };

        test(source, expected_input);
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

        let expected_input = Input {
            kind: vec![
                EndLine, WhiteSpace, FunctionKw, WhiteSpace, Identifier, LParen, Identifier,
                RParen, WhiteSpace, LCurly, EndLine, WhiteSpace, VarKw, WhiteSpace, Identifier,
                WhiteSpace, Assign, WhiteSpace, Number, Semicolon, EndLine, WhiteSpace, VarKw,
                WhiteSpace, Identifier, WhiteSpace, Assign, WhiteSpace, Number, Semicolon, EndLine,
                WhiteSpace, WhileKw, WhiteSpace, LParen, Identifier, Sub, Number, LessThan,
                Identifier, RParen, WhiteSpace, LCurly, EndLine, WhiteSpace, Identifier, Add, Add,
                Semicolon, EndLine, WhiteSpace, Identifier, WhiteSpace, Mul, Assign, WhiteSpace,
                Number, Semicolon, EndLine, WhiteSpace, RCurly, EndLine, WhiteSpace, ReturnKw,
                WhiteSpace, Identifier, Semicolon, EndLine, WhiteSpace, RCurly,
            ],
            source: &source,
            position: vec![
                0..1,
                1..5,
                5..13,
                13..14,
                14..19,
                19..20,
                20..21,
                21..22,
                22..23,
                23..24,
                24..25,
                25..33,
                33..36,
                36..37,
                37..38,
                38..39,
                39..40,
                40..41,
                41..42,
                42..43,
                43..44,
                44..52,
                52..55,
                55..56,
                56..57,
                57..58,
                58..59,
                59..60,
                60..61,
                61..62,
                62..63,
                63..71,
                71..76,
                76..77,
                77..78,
                78..79,
                79..80,
                80..81,
                81..82,
                82..83,
                83..84,
                84..85,
                85..86,
                86..87,
                87..99,
                99..100,
                100..101,
                101..102,
                102..103,
                103..104,
                104..116,
                116..117,
                117..118,
                118..119,
                119..120,
                120..121,
                121..122,
                122..123,
                123..124,
                124..132,
                132..133,
                133..134,
                134..142,
                142..148,
                148..149,
                149..150,
                150..151,
                151..152,
                152..156,
                156..157,
            ],
        };

        test(source, expected_input);
    }

    // #[test]
    // fn test_gen() {
    //     let source = r#"
    // "#;

    //     let input = Input::new(&source);
    //     println!("{:?}", input.kind);
    //     println!("{:?}", input.position);
    // }
}
