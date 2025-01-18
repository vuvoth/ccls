use crate::token_kind::TokenKind;

#[derive(Debug, Clone)]
pub enum Event {
    Open { kind: TokenKind },
    Close,
    TokenPosition(usize),
    ErrorReport(String),
}
