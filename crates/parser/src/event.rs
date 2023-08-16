use crate::{node::Token, token_kind::TokenKind};

#[derive(Debug, Clone)]
pub enum Event {
    Open { kind: TokenKind },
    Close,
    Token(Token),
}
