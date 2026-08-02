use parser::token_kind::TokenKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CircomLanguage {}

impl rowan::Language for CircomLanguage {
    type Kind = TokenKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        debug_assert!(
            raw.0 <= TokenKind::__LAST as u16,
            "raw syntax kind out of range for TokenKind"
        );
        // SAFETY: `TokenKind` is declared `#[repr(u16)]` with `Error = 0` and every subsequent
        // variant auto-incremented by one (no explicit discriminants, hence no gaps) up to the
        // `__LAST` sentinel. Its memory layout is therefore exactly a `u16` with the same numeric
        // representation as a direct cast, which is enforced at compile time by the
        // `size_of::<TokenKind>() == 2` const assertion below. The value is bounded by `__LAST`
        // (matching `token_kind::From<u16>`), and the `debug_assert!` above rejects foreign raw
        // values in debug builds. `transmute` is therefore sound.
        unsafe { std::mem::transmute::<u16, TokenKind>(raw.0) }
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

// Compile-time guarantee that the `transmute` above reinterprets a 2-byte value, matching the
// `#[repr(u16)]` layout of `TokenKind`.
const _: () = assert!(std::mem::size_of::<TokenKind>() == 2);

pub type SyntaxNode = rowan::SyntaxNode<CircomLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<CircomLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<CircomLanguage>;
pub type SyntaxNodeChildren = rowan::SyntaxNodeChildren<CircomLanguage>;
pub type SyntaxElementChildren = rowan::SyntaxElementChildren<CircomLanguage>;
pub type PreorderWithTokens = rowan::api::PreorderWithTokens<CircomLanguage>;
