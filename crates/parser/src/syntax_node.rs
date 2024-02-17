use crate::token_kind::TokenKind;

impl From<TokenKind> for rowan::SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CircomLang {}

impl rowan::Language for CircomLang {
    type Kind = TokenKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= TokenKind::ROOT as u16);
        unsafe { std::mem::transmute::<u16, TokenKind>(raw.0) }
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<CircomLang>;
pub type SyntaxToken = rowan::SyntaxToken<CircomLang>;
pub type SyntaxElement = rowan::SyntaxElement<CircomLang>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<CircomLang>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<CircomLang>;
pub type PreorderWithTokens = rowan::api::PreorderWithTokens<CircomLang>;
