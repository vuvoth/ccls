use crate::{event::Event, grammar::entry::Scope, lexer::Token, token_kind::TokenKind};

pub struct Parser<'a> {
    pub(crate) tokens: &'a [Token<'a>],
    pos: usize,
    fuel: u32,
    /// Current expression-nesting depth (incremented in the Pratt recursive core). Bounds recursion
    /// on deeply-nested untrusted input so a pathological `.circom` file cannot overflow the stack;
    /// the `fuel` guard only catches *non-advancing* loops, not *deep* ones.
    pub(crate) depth: u32,
    pub(crate) events: Vec<Event>,
}

#[derive(Clone, Copy, Debug)]
pub enum Marker {
    Open(usize),
    Close(usize),
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token<'a>]) -> Self {
        Self {
            tokens,
            pos: 0,
            fuel: 256,
            depth: 0,
            events: Vec::new(),
        }
    }

    /// Kind of the token at `pos`, or [`TokenKind::EOF`] past the end of input.
    fn kind_of(&self, pos: usize) -> TokenKind {
        self.tokens.get(pos).map_or(TokenKind::EOF, |t| t.kind)
    }

    /// Emit leading trivia (whitespace/comments) at and after the current position into the event
    /// stream, stopping at the first non-trivial token; returns its kind, or `TokenKind::EOF`.
    fn wrap_trivia(&mut self) -> TokenKind {
        loop {
            let kind = self.kind_of(self.pos);

            if !kind.is_trivial() {
                return kind;
            }

            self.fuel = 256;
            self.events.push(Event::Token(self.pos));
            self.skip();
        }
    }

    /// The token at the current position after skipping leading trivia.
    pub fn current(&mut self) -> TokenKind {
        self.wrap_trivia()
    }

    /// Lookahead without side effects: the kind of the `n`-th non-trivial token at/after the current
    /// position (`n == 0` is the current token), or `EOF` past the end. Unlike `current`, this does
    /// NOT emit trivia into the event stream (it peeks via `kind_of`), so it is safe to call for
    /// dispatch decisions before/after `current` without double-emitting trivia.
    pub fn nth(&self, n: usize) -> TokenKind {
        let mut pos = self.pos;
        let mut remaining = n;
        loop {
            let kind = self.kind_of(pos);
            if kind.is_trivial() {
                pos += 1;
                continue;
            }
            if remaining == 0 {
                return kind;
            }
            remaining -= 1;
            pos += 1;
        }
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

    pub fn open(&mut self) -> Marker {
        if !self.events.is_empty() {
            self.wrap_trivia();
        }

        let marker = Marker::Open(self.events.len());
        self.events.push(Event::Open {
            kind: TokenKind::Error,
        });
        marker
    }

    /// Open a new marker immediately *before* an already-closed one, so the new node wraps the
    /// closed node's subtree (rust-analyzer calls this `precede`).
    pub fn precede(&mut self, marker: Marker) -> Marker {
        match marker {
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

    /// Record the current token into the event stream, then advance past it.
    pub fn advance(&mut self) {
        self.fuel = 256;
        let token = Event::Token(self.pos);
        self.events.push(token);
        self.skip();
    }

    /// Advance past the current token without recording it. The `fuel` counter is the infinite-loop
    /// guard: it is reset to 256 each time an event is emitted (`advance`/`wrap_trivia`) and
    /// decremented here; hitting zero means the parser consumed 256 tokens without progress, i.e. a
    /// grammar bug.
    pub fn skip(&mut self) {
        if self.fuel == 0 {
            // Fuel exhaustion means the parser consumed 256 tokens without emitting an event — a
            // grammar bug (e.g. a loop that never advances), not user input.
            panic!("parser made no progress (fuel exhausted); likely a grammar bug");
        }
        self.fuel -= 1;
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    pub fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            return true;
        }

        false
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

    /// Wrap the bad current token in an `Error` node and advance past it (error recovery).
    pub fn advance_with_error(&mut self) {
        let marker = self.open();
        // TODO: Error reporting.
        if !self.eof() {
            self.advance();
        }
        self.close(marker, TokenKind::Error);
    }

    pub(crate) fn error_report(&mut self, error: String) {
        let marker = self.open();

        let token = Event::ErrorReport(error);
        self.events.push(token);

        self.close(marker, TokenKind::Error);
    }

    pub fn parse_with_scope(tokens: &[Token], scope: Scope) -> Vec<Event> {
        let mut p = Parser::new(tokens);
        scope.parse(&mut p);
        p.events
    }

    pub fn parse(tokens: &[Token]) -> Vec<Event> {
        let c = Scope::CircomProgram;
        Parser::parse_with_scope(tokens, c)
    }
}
