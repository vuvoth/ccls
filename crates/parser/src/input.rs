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

    pub fn token_value(&self, index: usize) -> &'a str {
        if index < self.kind.len() {
            &self.source[self.position[index].start..self.position[index].end]
        } else {
            // return error for out of bound index
            ""
        }
    }

    pub fn kind_of(&self, index: usize) -> TokenKind {
        if index < self.kind.len() {
            self.kind[index]
        } else {
            TokenKind::EOF
        }
    }

    pub fn position_of(&self, index: usize) -> Range<usize> {
        if index < self.kind.len() {
            self.position[index].clone()
        } else {
            // return error for out of bound index
            0..0
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

    #[test]
    fn test_input() {
        let source = r#"
        /*a + b == 10*/
        a + 10
    "#
        .to_string();

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
                TokenKind::WhiteSpace
            ],
            source: &source,
            position: vec![
                {0..1},
                {1..9},
                {9..24},
                {24..25},
                {25..33},
                {33..34},
                {34..35},
                {35..36},
                {36..37},
                {37..39},
                {39..40},
                {40..44},
            ]
        };

        let input = Input::new(&source);

        assert_eq!(expected_input, input, "Tokens extract from source code are not correct");

        // test size method
        let expected_size = input.kind.len();
        let size = input.size();
        assert_eq!(expected_size, size, "size method failed");

        // test methods with index out of bound
        let index = input.kind.len();

        let expected_token_value = "";
        let token_value = input.token_value(index);
        assert_eq!(expected_token_value, token_value, "token_value failed (case: index out of bound)");

        let expected_kind = TokenKind::EOF;
        let kind = input.kind_of(index);
        assert_eq!(expected_kind, kind, "kind_of failed (case: index out of bound)");

        let expected_position = 0..0;
        let position = input.position_of(index);
        assert_eq!(expected_position, position, "position_of failed (case: index out of bound)");

        // test methods with index in bound
        if input.size() == 0 {
            return;
        }

        let index = input.size() / 2; // a valid index if input size > 0
        
        let expected_token_value = &input.source[input.position[index].clone()];
        let token_value = input.token_value(index);
        assert_eq!(expected_token_value, token_value, "token_value failed");

        let expected_kind = input.kind[index];
        let kind = input.kind_of(index);
        assert_eq!(expected_kind, kind, "kind_of failed");
       
        let expected_position = input.position[index].clone();
        let position = input.position_of(index);
        assert_eq!(expected_position, position, "position_of failed");
        
    }
}
