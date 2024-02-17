use crate::token_kind::TokenKind;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Open { kind: TokenKind },
    Close,
    TokenPosition(usize),
}
