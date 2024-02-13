use std::cell::Cell;

use logos::Lexer;
use lsp_types::Position;

use crate::{
    event::Event,
    grammar::entry::Scope,
    node::{Child, Token, Tree},
    token_kind::TokenKind,
};

pub struct Parser<'a> {
    lexer: Lexer<'a, TokenKind>,
    pos: usize,
    bootstrap: bool,
    last_position: Position,
    current_token: Token,
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
        let token = Event::Token(self.current());
        self.events.push(token);
        self.skip();
    }

    pub fn advance_with_token(&mut self, token: Token) {
        // assert!(token.kind != TokenKind::EOF);
        if token.kind != TokenKind::EOF {
            self.fuel.set(256);
            let token = Event::Token(token);
            self.events.push(token);
        }
    }

    pub fn advance_with_error(&mut self, error: &str) {
        let m = self.open();
        // TODO: Error reporting.
        if !self.eof() {
            self.advance();
        } else {
            self.events.push(Event::Token(Token {
                kind: TokenKind::EOF,
                text: "".to_string(),
                range: lsp_types::Range {
                    start: self.last_position,
                    end: self.last_position,
                },
            }))
        }
        self.close(m, TokenKind::Error);
    }

    pub fn build_tree(self) -> Result<Tree, ParserError> {
        let mut events = self.events;
        let mut stack = Vec::new();

        if !matches!(events.pop(), Some(Event::Close)) {
            return Err(ParserError::InvalidEvents);
        }

        for event in events {
            match event {
                Event::Open { kind } => {
                    stack.push(Tree {
                        kind,
                        children: Vec::new(),
                    });
                }
                Event::Close => {
                    let tree = stack.pop().unwrap();
                    if stack.last_mut().is_none() {
                        return Err(ParserError::InvalidEvents);
                    } else {
                        stack.last_mut().unwrap().children.push(Child::Tree(tree));
                    }
                }
                Event::Token(token) => {
                    if stack.last().is_some() {
                        stack.last_mut().unwrap().children.push(Child::Token(token));
                    } else {
                        return Err(ParserError::InvalidEvents);
                    }
                }
            }
        }

        if stack.is_empty() {
            return Err(ParserError::InvalidEvents);
        }
        let tree = stack.pop().unwrap();

        if !stack.is_empty() {
            return Err(ParserError::InvalidEvents);
        }
        Ok(tree)
    }
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            lexer: Lexer::<TokenKind>::new(source),
            pos: 0,
            current_token: Token {
                kind: TokenKind::EOF,
                text: "".to_string(),
                range: lsp_types::Range::default(),
            },
            last_position: Position::new(0, 0),
            bootstrap: false,
            fuel: Cell::new(256),
            events: Vec::new(),
        }
    }

    pub fn current(&mut self) -> Token {
        return self.current_token.clone();
    }

    pub fn next(&mut self) -> TokenKind {
        self.bootstrap = true;

        let mut kind = self.lexer.next().unwrap_or(TokenKind::EOF);
        self.current_token = Token::new(
            kind,
            self.lexer.slice(),
            self.lexer.span(),
            self.last_position,
        );
        self.last_position = self.current_token.range.end;

        while self.current().kind.is_travial() {
            kind = self.lexer.next().unwrap_or(TokenKind::EOF);
            let m = self.open();
            // skip travial token
            self.advance_with_token(self.current_token.clone());
            self.close(m, TokenKind::WhiteSpace);
            self.current_token = Token::new(
                kind,
                self.lexer.slice(),
                self.lexer.span(),
                self.last_position,
            );
            self.last_position = self.current_token.range.end;
        }
        self.current_token.kind
    }

    pub fn at(&mut self, kind: TokenKind) -> bool {
        let token = self.current();
        token.kind == kind
    }

    pub fn at_any(&mut self, kinds: &[TokenKind]) -> bool {
        let current_kind = self.current().kind;
        return kinds.contains(&current_kind);
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
        if self.bootstrap == false {
            self.next();
        }
        if self.at(kind) {
            let token = self.current().clone();
            self.events.push(Event::Token(token));
            self.skip();
            return true;
        }
        return false;
    }

    pub fn expect_any(&mut self, kinds: &[TokenKind]) {
        let kind = self.current().kind;
        if kinds.contains(&kind) {
            self.advance();
            return;
        }
    }
    pub fn expect(&mut self, kind: TokenKind) {
        if self.at(kind) {
            self.advance();
        }
    }

    pub fn eof(&mut self) -> bool {
        self.current().kind == TokenKind::EOF
    }
}

impl Parser<'_> {
    pub fn parse(&mut self, scope: Scope) {
        let m = self.open();

        if !self.bootstrap {
            self.next();
        }

        scope.parse(self);
        self.close(m, TokenKind::Circom);
    }

    pub fn parse_source(source: &str) -> Result<Tree, ParserError> {
        let mut p = Parser::new(source);
        p.parse(Scope::CircomProgram);
        p.build_tree()
    }
}

#[cfg(test)]
mod tests {
    use lsp_types::Position;

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

        let cst = Parser::parse_source(&source);

        println!("{:?}", cst.ok().unwrap());
    }

    #[test]
    fn other_parser_test() {
        let source: String = 
r#"pragma circom 2.0.0;

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

        let cst = Parser::parse_source(&source);

        let tree = cst.unwrap();
      
        let t =  tree.clone().lookup_element_by_range(Position {
            line: 7, 
            character: 12
        });

        let tmp = tree.lookup_definition(t.unwrap());
        println!("{:?}", tmp);
    }
}
