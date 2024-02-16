use crate::{
    syntax_node::{SyntaxNode, SyntaxToken},
    token_kind::TokenKind,
};

pub trait AstNode {
    fn can_cast(token_kind: TokenKind) -> bool;

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized;

    fn syntax(&self) -> &SyntaxNode;
}

pub trait AstToken {
    fn can_cast(token: TokenKind) -> bool
    where
        Self: Sized;

    fn cast(syntax: SyntaxToken) -> Option<Self>
    where
        Self: Sized;

    fn syntax(&self) -> &SyntaxNode;
}

pub struct Block {
    syntax: SyntaxNode,
}

pub struct IfStatement {
    syntax: SyntaxNode,
}

#[derive(Debug, Clone)]
pub struct Version {
    syntax: SyntaxNode,
}

impl AstNode for Version {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::Version
    }
    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            return Some(Version { syntax });
        }
        None
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

#[derive(Debug, Clone)]
pub struct PragmaDef {
    syntax: SyntaxNode,
}

impl AstNode for PragmaDef {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::Pragma
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            return Some(Self { syntax });
        }
        None
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl PragmaDef {
    pub fn version(&self) -> Option<Version> {
        self.syntax.children().find_map(Version::cast)
    }
}
