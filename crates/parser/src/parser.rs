use std::cell::Cell;

use crate::{
    event::Event, grammar::entry::Scope, input::Input, output::Output, token_kind::TokenKind,
};

pub struct Context {
    pub r_curly_count: i32,
}

pub struct Parser<'a> {
    pub(crate) input: &'a Input<'a>,
    pub context: Context,
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
    pub fn wrap_trivial_tokens(&mut self) -> TokenKind {
        loop {
            let kind = self.input.kind_of(self.pos);

            if kind.is_trivial() == false {
                return kind;
            }

            self.events.push(Event::Open { kind });

            self.fuel.set(256);
            self.events.push(Event::TokenPosition(self.pos));
            self.skip();

            self.events.push(Event::Close);
        }
    }

    pub fn open(&mut self) -> Marker {
        if self.events.len() > 0 {
            self.wrap_trivial_tokens();
        }

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

    pub fn close(&mut self, marker_open: Marker, kind: TokenKind) -> Marker {
        match marker_open {
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

    pub fn advance_with_error(&mut self, _error: &str) {
        let m = self.open();
        // TODO: Error reporting.
        if !self.eof() {
            self.advance();
        }
        self.close(m, TokenKind::Error);
    }

    pub fn error_report(&mut self, error: String) {
        let m = self.open();

        let token = Event::ErrorReport(error);
        self.events.push(token);

        self.close(m, TokenKind::Error);
    }
}

impl<'a> Parser<'a> {
    pub fn new(input: &'a Input) -> Self {
        Self {
            input,
            pos: 0,
            context: Context { r_curly_count: 0 },
            fuel: Cell::new(256),
            events: Vec::new(),
        }
    }

    pub fn inc_rcurly(&mut self) {
        self.context.r_curly_count += 1;
    }

    pub fn dec_rcurly(&mut self) {
        self.context.r_curly_count += 1;
    }

    pub fn current(&mut self) -> TokenKind {
        self.wrap_trivial_tokens()
    }

    pub fn next(&mut self) -> TokenKind {
        if self.fuel.get() == 0 {
            panic!("parser is stuck");
        }
        self.fuel.set(self.fuel.get() - 1);
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
            self.advance();
            return true;
        }

        false
    }

    pub fn expect_any(&mut self, kinds: &[TokenKind]) {
        let kind = self.current();
        if kinds.contains(&kind) {
            self.advance();
        } else {
            let error = format!("expect {:?} but got {:?}", kinds, kind);
            self.error_report(error);
        }
    }

    pub fn expect(&mut self, kind: TokenKind) {
        if self.at(kind) {
            self.advance();
        } else {
            let error = format!("expect {:?} but got {:?}", kind, self.current());
            self.error_report(error);
        }
    }

    pub fn eof(&mut self) -> bool {
        self.current() == TokenKind::EOF
    }
}

impl Parser<'_> {
    pub fn parsing_with_scope(input: &Input, scope: Scope) -> Output {
        let mut p = Parser::new(input);
        scope.parse(&mut p);
        Output::from(p.events)
    }

    pub fn parsing(input: &Input) -> Output {
        let c = Scope::CircomProgram;
        Parser::parsing_with_scope(input, c)
    }
}
