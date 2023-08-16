use crate::{node::Token, token_kind::TokenKind};

#[derive(Debug, Clone, Copy)]
pub enum Event<'a> {
    Open { kind: TokenKind },
    Close,
    Token(Token<'a>),
}
