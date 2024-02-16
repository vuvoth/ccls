use std::{cell::Cell, usize::MAX};

use rowan::GreenNode;

use crate::{
    event::{self, Event},
    grammar::entry::Scope,
    input::Input,
    node::{Child, Tree},
    syntax::{covert_to_tree_format, CircomParser},
    token_kind::TokenKind,
};

pub struct Parser<'a> {
    pub(crate) input: &'a Input<'a>,
    pos: usize,
    fuel: Cell<u32>,
    pub(crate) events: Vec<Event>,
}

#[derive(Clone, Copy, Debug)]
pub enum Marker {
    Open(usize),
    Close(usize),
}

#[derive(Debug)]
pub enum ParserError {
    InvalidEvents,
}

impl<'a> Parser<'a> {
    pub fn open(&mut self) -> Marker {
        let marker = Marker::Open(self.events.len());
        self.events.push(Event::Open {
            kind: TokenKind::Error,
        });
        marker
    }

    pub fn open_before(&mut self, marker_closed: Marker) -> Marker {
        match marker_closed {
            Marker::Close(index) => {
                let marker_opened = Marker::Open(index);
                self.events.insert(
                    index,
                    Event::Open {
                        kind: TokenKind::EOF,
                    },
                );
                marker_opened
            }
            _ => unreachable!(),
        }
    }

    pub fn close(&mut self, marker_close: Marker, kind: TokenKind) -> Marker {
        match marker_close {
            Marker::Open(index) => {
                self.events[index] = Event::Open { kind };
                self.events.push(Event::Close);
                Marker::Close(index)
            }
            _ => unreachable!(),
        }
    }

    pub fn advance(&mut self) {
        // assert!(!self.eof());
        self.fuel.set(256);
        let token = Event::TokenPosition(self.pos);
        self.events.push(token);
        self.skip();
    }

    pub fn advance_with_token(&mut self, index: usize) {
        // assert!(token.kind != TokenKind::EOF);
        if self.input.kind_of(index) != TokenKind::EOF {
            self.fuel.set(256);
            let token = Event::TokenPosition(index);
            self.events.push(token);
        }
    }

    pub fn advance_with_error(&mut self, error: &str) {
        let m = self.open();
        // TODO: Error reporting.
        if !self.eof() {
            self.advance();
        } else {
            // TODO: make this logic more sense
            self.events.push(Event::TokenPosition(MAX))
        }
        self.close(m, TokenKind::Error);
    }
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a Input) -> Self {
        Self {
            input,
            pos: 0,
            fuel: Cell::new(256),
            events: Vec::new(),
        }
    }

    pub fn current(&mut self) -> TokenKind {
        // we skip whitespace token
        // TODO: process this for format
        while self.input.kind_of(self.pos).is_travial() {
            self.advance();
        }
        self.input.kind_of(self.pos)
    }

    pub fn next(&mut self) -> TokenKind {
        if self.pos < self.input.size() {
            self.pos += 1;
            return self.input.kind_of(self.pos);
        }

        TokenKind::EOF
    }

    pub fn at(&mut self, kind: TokenKind) -> bool {
        self.current() == kind
    }

    pub fn at_any(&mut self, kinds: &[TokenKind]) -> bool {
        let current_kind = self.current();
        kinds.contains(&current_kind)
    }

    pub fn skip(&mut self) {
        self.next();
    }

    pub fn skip_if(&mut self, kinds: &[TokenKind]) {
        if self.at_any(kinds) {
            self.skip();
        }
    }

    pub fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.events.push(Event::TokenPosition(self.pos));
            self.skip();
            return true;
        }
        false
    }

    pub fn expect_any(&mut self, kinds: &[TokenKind]) {
        let kind = self.current();
        if kinds.contains(&kind) {
            self.advance();
        }
    }
    pub fn expect(&mut self, kind: TokenKind) {
        if self.at(kind) {
            self.advance();
        } else {
            println!("expect {:?} but got {:?}", kind, self.current());
        }
    }

    pub fn eof(&mut self) -> bool {
        self.current() == TokenKind::EOF
    }
}

impl Parser<'_> {
    pub fn parse(&mut self, scope: Scope) {
        scope.parse(self);
    }

    pub fn parse_circom(source: &str) -> GreenNode {
        Self::parse_scope(source, Scope::CircomProgram)
    }

    pub fn parse_scope(source: &str, scope: Scope) -> GreenNode {
        let input = Input::new(source);
        let mut p = Parser::new(&input);
        p.parse(scope);
        let mut builder = CircomParser::new(&input);
        let tree = covert_to_tree_format(&mut p.events);
        builder.build(tree);
        builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use rowan::SyntaxNode;

    use crate::syntax_node::CircomLang;

    use super::Parser;

    #[test]
    fn test_parser() {
        let source: String = r#"
        pragma circom 2.0.1;

        template Adder() {
            signal input x;
            signal input y;   
            signal input y; 
            sign
        } 



"#
        .to_string();

        let cst = Parser::parse_circom(&source);
    }

    #[test]
    fn other_parser_test() {
        let source: String = r#"pragma circom 2.0.0;

        
        template Multiplier2 () {  
        
           // Declaration of signals.  
           signal input a;  
           signal input b;  
           signal output c;  
        
           // Constraints.  
           c <== a * b;  
        }
        "#
        .to_string();

        let green_node = Parser::parse_circom(&source);
        let syntax_node = SyntaxNode::<CircomLang>::new_root(green_node.clone());

        // find token
    }
}
