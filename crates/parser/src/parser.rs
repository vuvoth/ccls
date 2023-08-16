use std::cell::Cell;

use logos::Lexer;

use crate::{
    event::Event,
    grammar::entry::Scope,
    node::{Child, Token, Tree},
    token_kind::TokenKind,
};

pub struct Parser<'a> {
    lexer: Lexer<'a, TokenKind>,
    pos: usize,
    current_token: Token,
    fuel: Cell<u32>,
    pub(crate) events: Vec<Event>,
}

#[derive(Clone, Copy, Debug)]
pub enum Marker {
    Open(usize),
    Close(usize),
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
        assert!(!self.eof());
        self.fuel.set(256);
        let token = Event::Token(self.current());
        self.events.push(token);
        self.skip();
    }

    pub fn advance_with_error(&mut self, error: &str) {
        println!("{:?}",self.current().kind);
        let m = self.open();
        // TODO: Error reporting.
        eprintln!("{error}");
        if !self.eof() {
            self.advance();
        } else {
            self.events.push(Event::Token(Token {
                kind: TokenKind::EOF,
                text: "".to_string(),
            }))
        }
        self.close(m, TokenKind::Error);
    }

    pub fn build_tree(self) -> Tree {
        let mut events = self.events;
        let mut stack = Vec::new();
        assert!(matches!(events.pop(), Some(Event::Close)));

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
                    stack.last_mut().unwrap().children.push(Child::Tree(tree));
                }
                Event::Token(token) => {
                    stack.last_mut().unwrap().children.push(Child::Token(token));
                }
            }
        }

        let tree = stack.pop().unwrap();
        assert!(stack.is_empty());
        tree
    }
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut parser = Self {
            lexer: Lexer::<TokenKind>::new(source),
            pos: 0,
            current_token: Token {
                kind: TokenKind::Start,
                text: "".to_string(),
            },
            fuel: Cell::new(256),
            events: Vec::new(),
        };
        parser.next();
        println!("{:?}", parser.current());
        parser
    }

    pub fn current(&mut self) -> Token {
        return self.current_token.clone();
    }

    pub fn next(&mut self) -> TokenKind {
        let kind = self.lexer.next().unwrap_or(TokenKind::EOF);
        self.current_token = Token::new(kind, self.lexer.slice());
        kind
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
        eprintln!("expected skip {kinds:?}");
    }

    pub fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            let text = self.lexer.slice();
            self.events.push(Event::Token(Token::new(kind, text)));
            self.skip();
            return true;
        }
        return false;
    }

    pub fn expect_any(&mut self, kinds: &[TokenKind]) {
        let kind = self.current().kind;
        if kinds.contains(&kind) {
            self.eat(kind);
            return;
        }
        eprintln!("expected in any {kinds:?}");
    }
    pub fn expect(&mut self, kind: TokenKind) {
        if self.eat(kind) {
            return;
        }
        eprintln!("expected {kind:?}");
    }

    pub fn eof(&mut self) -> bool {
        self.current().kind == TokenKind::EOF
    }
}

impl Parser<'_> {
    pub fn parse(&mut self, scope: Scope) {
        scope.parse(self);
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;
    use super::Scope;

    #[test]
    fn test_parser() {
        let source: String = r#"
            pragma circom 2.0.1;
            include "another_template"; 
        "#
        .to_string();
        let mut parser = Parser::new(&source);

        parser.parse(Scope::CircomProgram);

        let cst = parser.build_tree();

        println!("{:?}", cst);
    }
}
