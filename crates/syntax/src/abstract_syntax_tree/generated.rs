use crate::syntax_node::{CircomLanguage, SyntaxKind, SyntaxNode, SyntaxToken};
use parser::{Rule, Token};
use rowan::ast::AstNode;

/// Macro to reduce boilerplate for AST nodes
macro_rules! define_node {
    ($(#[$doc:meta])* $name:ident, $kind:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(pub(crate) SyntaxNode);

        impl AstNode for $name {
            type Language = CircomLanguage;
            fn can_cast(kind: SyntaxKind) -> bool { kind == $kind }
            fn cast(syntax: SyntaxNode) -> Option<Self> {
                Self::can_cast(syntax.kind()).then_some(Self(syntax))
            }
            fn syntax(&self) -> &SyntaxNode { &self.0 }
        }

        impl $name {
            pub fn syntax_node(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

/// Macro to reduce boilerplate for token wrappers
macro_rules! define_token {
    ($(#[$doc:meta])* $name:ident, $kind:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(pub(crate) SyntaxToken);

        impl $name {
            pub fn syntax_token(&self) -> &SyntaxToken {
                &self.0
            }

            pub fn text(&self) -> &str {
                self.0.text()
            }

            pub fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }

            pub fn cast(token: SyntaxToken) -> Option<Self> {
                Self::can_cast(token.kind()).then_some(Self(token))
            }
        }
    };
}

// Helper function to find a token child of a specific kind
pub fn token_child<T: crate::abstract_syntax_tree::TokenCast>(parent: &SyntaxNode) -> Option<T> {
    parent
        .children_with_tokens()
        .find_map(|elem| elem.into_token().and_then(T::cast))
}

/// Trait for token types that can be cast from SyntaxToken
pub trait TokenCast: Sized {
    fn cast(token: SyntaxToken) -> Option<Self>;
}

// Implement TokenCast for our token types
impl TokenCast for Ident {
    fn cast(token: SyntaxToken) -> Option<Self> {
        Self::cast(token)
    }
}

impl TokenCast for StringLiteral {
    fn cast(token: SyntaxToken) -> Option<Self> {
        Self::cast(token)
    }
}

impl TokenCast for Number {
    fn cast(token: SyntaxToken) -> Option<Self> {
        Self::cast(token)
    }
}

impl TokenCast for Version {
    fn cast(token: SyntaxToken) -> Option<Self> {
        Self::cast(token)
    }
}

// ============ Program Structure ============

define_node!(
    /// Root node of a Circom program
    Program, SyntaxKind::from_rule(Rule::Program)
);

define_node!(
    /// Pragma directive: `pragma circom 2.0.0;`
    Pragma, SyntaxKind::from_rule(Rule::Pragma)
);

define_node!(
    /// Include directive: `include "path";`
    Include, SyntaxKind::from_rule(Rule::Include)
);

define_node!(
    /// Template definition: `template Name(params) { ... }`
    Template, SyntaxKind::from_rule(Rule::Template)
);

define_node!(
    /// Function definition: `function Name(params) { ... }`
    Function, SyntaxKind::from_rule(Rule::Function)
);

// ============ Declarations ============

define_node!(
    /// Signal direction (input/output)
    SignalIo, SyntaxKind::from_rule(Rule::SignalIo)
);

define_node!(
    /// Signal declaration rest: `[io] [tags] signals;`
    SignalDecl, SyntaxKind::from_rule(Rule::SignalDeclRest)
);

define_node!(
    /// Variable declaration: `var x [= expr];`
    VarDecl, SyntaxKind::from_rule(Rule::VarDeclRest)
);

define_node!(
    /// Component declaration: `component x = Template(args);`
    ComponentDecl, SyntaxKind::from_rule(Rule::ComponentDeclRest)
);

// ============ Expressions & Calls ============

define_node!(
    /// Template instantiation: `TemplateName(args)`
    TemplateCall, SyntaxKind::from_rule(Rule::TemplateCall)
);

define_node!(
    /// Block: `{ statements }`
    Block, SyntaxKind::from_rule(Rule::Block)
);

define_node!(
    /// Statement wrapper
    Stmt, SyntaxKind::from_rule(Rule::Stmt)
);

// ============ Parameters & Arguments ============

define_node!(
    /// Parameter list: `(a, b, c)`
    ParamList, SyntaxKind::from_rule(Rule::ParamList)
);

define_node!(
    /// Argument list: `(expr, expr)`
    ArgList, SyntaxKind::from_rule(Rule::ArgList)
);

// ============ Identifiers & Literals ============

define_token!(
    /// Identifier token
    Ident, SyntaxKind::from_token(Token::Identifier)
);

define_node!(
    /// Complex identifier (with optional array subscripts)
    ComplexId, SyntaxKind::from_rule(Rule::ComplexId)
);

define_token!(
    /// String literal token
    StringLiteral, SyntaxKind::from_token(Token::String)
);

define_token!(
    /// Number literal
    Number, SyntaxKind::from_token(Token::Number)
);

define_token!(
    /// Version literal (in pragma)
    Version, SyntaxKind::from_token(Token::Version)
);
