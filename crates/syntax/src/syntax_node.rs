use parser::{Rule, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyntaxKind(pub u16);

impl SyntaxKind {
    pub const fn from_token(token: Token) -> Self {
        SyntaxKind(token as u16)
    }

    pub const fn from_rule(rule: Rule) -> Self {
        SyntaxKind((rule as u16) | 0x8000)
    }

    pub fn is_token(&self) -> bool {
        self.0 & 0x8000 == 0
    }

    pub fn is_rule(&self) -> bool {
        self.0 & 0x8000 != 0
    }

    pub fn to_token(&self) -> Option<Token> {
        if self.is_token() {
            unsafe { std::mem::transmute::<u8, Token>(self.0 as u8) }.into()
        } else {
            None
        }
    }

    pub fn to_rule(&self) -> Option<Rule> {
        if self.is_rule() {
            unsafe { std::mem::transmute::<u8, Rule>((self.0 & !0x8000) as u8) }.into()
        } else {
            None
        }
    }
}

impl From<Token> for SyntaxKind {
    fn from(token: Token) -> Self {
        SyntaxKind::from_token(token)
    }
}

impl From<Rule> for SyntaxKind {
    fn from(rule: Rule) -> Self {
        SyntaxKind::from_rule(rule)
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        rowan::SyntaxKind(kind.0)
    }
}

impl From<rowan::SyntaxKind> for SyntaxKind {
    fn from(raw: rowan::SyntaxKind) -> Self {
        SyntaxKind(raw.0)
    }
}

impl std::fmt::Display for SyntaxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(token) = self.to_token() {
            write!(f, "{:?}", token)
        } else if let Some(rule) = self.to_rule() {
            write!(f, "{:?}", rule)
        } else {
            write!(f, "Unknown({})", self.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CircomLanguage {}

impl rowan::Language for CircomLanguage {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        raw.into()
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

// ============ Helper functions for AST analysis ============

/// Check if a token represents a callable reference (template or function).
///
/// This detects patterns like:
/// - `component m = Template()` - inside TemplateCall node
/// - `signal x <== Template()` - identifier followed by CallPostfix
/// - `var y = func()` - function call
pub fn is_callable_reference(token: &SyntaxToken) -> bool {
    // Case 1: Inside a TemplateCall node (component declarations)
    if token.parent_ancestors().any(|n| n.kind() == SyntaxKind::from_rule(Rule::TemplateCall)) {
        return true;
    }

    // Case 2: Inside a ComponentDecl node
    if token.parent_ancestors().any(|n| n.kind() == SyntaxKind::from_rule(Rule::ComponentDeclRest)) {
        return true;
    }

    // Case 3: Identifier followed by CallPostfix (inline template/function calls)
    // Pattern: `Template()` in expressions like `signal x <== Template()`
    // The token's parent is PrimaryExpr, and its next sibling in the Expr is CallPostfix
    if token.kind() == SyntaxKind::from_token(Token::Identifier) {
        // Walk up to find if there's a CallPostfix applied to this identifier
        for ancestor in token.parent_ancestors() {
            // Check if this expr has a CallPostfix as a sibling
            if ancestor.kind() == SyntaxKind::from_rule(Rule::Expr) {
                // Look for CallPostfix among siblings
                for sibling in ancestor.children() {
                    if sibling.kind() == SyntaxKind::from_rule(Rule::CallPostfix) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check if a token is an identifier token.
pub fn is_identifier_token(token: &SyntaxToken) -> bool {
    token.kind() == SyntaxKind::from_token(Token::Identifier)
}
