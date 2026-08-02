use std::cell::Cell;

use crate::{event::Event, grammar::entry::Scope, lexer::Token, token_kind::TokenKind};

pub struct Parser<'a> {
    pub(crate) tokens: &'a [Token<'a>],
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
            let kind = self.kind_of(self.pos);

            if !kind.is_trivial() {
                return kind;
            }

            self.fuel.set(256);
            self.events.push(Event::Token(self.pos));
            self.skip();
        }
    }

    pub fn open(&mut self) -> Marker {
        if !self.events.is_empty() {
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

    pub fn close(&mut self, open_marker: Marker, kind: TokenKind) -> Marker {
        match open_marker {
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
        let token = Event::Token(self.pos);
        self.events.push(token);
        self.skip();
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
    pub fn new(tokens: &'a [Token<'a>]) -> Self {
        Self {
            tokens,
            pos: 0,
            fuel: Cell::new(256),
            events: Vec::new(),
        }
    }

    /// Kind of the token at `pos`, or [`TokenKind::EOF`] past the end of input.
    fn kind_of(&self, pos: usize) -> TokenKind {
        self.tokens.get(pos).map_or(TokenKind::EOF, |t| t.kind)
    }

    /// The token at the current position after skipping leading trivia, or `None` at end of input.
    /// Grammar-facing accessor: gives callers the current token's text/range without index
    /// arithmetic. Trivia is wrapped (emitted into the event stream) exactly as `current()` does.
    pub fn current_token(&mut self) -> Option<&'a Token<'a>> {
        self.current();
        self.tokens.get(self.pos)
    }

    pub fn current(&mut self) -> TokenKind {
        self.wrap_trivial_tokens()
    }

    /// Advance one token and return its kind (named `bump` to avoid confusion with
    /// `Iterator::next`). Returns `TokenKind::EOF` once past the end of input.
    pub fn bump(&mut self) -> TokenKind {
        if self.fuel.get() == 0 {
            // Fuel exhaustion means the parser consumed 256 tokens without emitting an event — a
            // grammar bug (e.g. a loop that never advances), not user input.
            panic!("parser made no progress (fuel exhausted); likely a grammar bug");
        }
        self.fuel.set(self.fuel.get() - 1);
        if self.pos < self.tokens.len() {
            self.pos += 1;
            return self.kind_of(self.pos);
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

    pub fn at_assign_token(&mut self) -> bool {
        let current_kind = self.current();
        current_kind.is_assign_token()
    }

    pub fn at_inline_assign_signal(&mut self) -> bool {
        let current_kind = self.current();
        current_kind.is_inline_assign_signal()
    }

    pub fn at_var_assign(&mut self) -> bool {
        let current_kind = self.current();
        current_kind.is_var_assign()
    }

    pub fn skip(&mut self) {
        self.bump();
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
    pub fn parsing_with_scope(tokens: &[Token], scope: Scope) -> Vec<Event> {
        let mut p = Parser::new(tokens);
        scope.parse(&mut p);
        p.events
    }

    pub fn parsing(tokens: &[Token]) -> Vec<Event> {
        let c = Scope::CircomProgram;
        Parser::parsing_with_scope(tokens, c)
    }
}
