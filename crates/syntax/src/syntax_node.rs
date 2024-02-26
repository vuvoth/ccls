use parser::token_kind::TokenKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CircomLanguage {}

impl rowan::Language for CircomLanguage {
    type Kind = TokenKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= TokenKind::ROOT as u16);
        unsafe { std::mem::transmute::<u16, TokenKind>(raw.0) }
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<CircomLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<CircomLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<CircomLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<CircomLanguage>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<CircomLanguage>;
pub type PreorderWithTokens = rowan::api::PreorderWithTokens<CircomLanguage>;
