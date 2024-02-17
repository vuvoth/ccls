use std::ops::Range;

use logos::Lexer;

use crate::token_kind::TokenKind;

#[derive(Debug)]
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
            input.kind.push(tk);
            input.position.push(lex.span());
        }

        input
    }

    pub fn token_value(&self, index: usize) -> &'a str {
        &self.source[self.position[index].start..self.position[index].end]
    }

    pub fn kind_of(&self, index: usize) -> TokenKind {
        if index < self.kind.len() {
            self.kind[index]
        } else {
            TokenKind::EOF
        }
    }

    pub fn position_of(&self, index: usize) -> Range<usize> {
        self.position[index].clone()
    }

    pub fn size(&self) -> usize {
        self.kind.len()
    }
}

mod tests {
    use super::Input;

    #[test]
    fn test_input() {
        let source: String = r#"
        pragma circom 2.0.0;
        template X() {
           component x = Multiplier2()
        }
        
        template Multiplier2 () {  
        
           // Declaration of signals.  
           signal input a;  
           signal input b;  
           signal output c;  
        
        
           signal output d;
           // Constraints.  
           c <== a * b;  
        }
                "#
        .to_string();

        let input = Input::new(&source);

        for i in 0..10 {
            println!("kind = {:?}", input.kind[i]);
            println!("position {:?}", input.position[i]);
        }
    }
}
