use std::marker::PhantomData;

use crate::{
    syntax_node::{SyntaxNode, SyntaxNodeChildren, SyntaxToken},
    token_kind::{self, TokenKind},
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


macro_rules! ast_node {
    ($ast_name: ident, $kind: expr) => {
        #[derive(Debug, Clone)]
        pub struct $ast_name {
            syntax: SyntaxNode
        }
        impl AstNode for $ast_name {
            fn can_cast(token_kind: TokenKind) -> bool {
                token_kind == $kind
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
    };
}


#[derive(Debug, Clone)]
pub struct AstChildren<N> {
    inner: SyntaxNodeChildren,
    ph: PhantomData<N>,
}

impl<N> AstChildren<N> {
    fn new(parent: &SyntaxNode) -> Self {
        AstChildren {
            inner: parent.children(),
            ph: PhantomData,
        }
    }
}

impl<N: AstNode> Iterator for AstChildren<N> {
    type Item = N;
    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}


#[derive(Debug, Clone)]
pub struct Statement {
    syntax: SyntaxNode,
}

ast_node!(AstStatementList, TokenKind::StatementList);


impl AstStatementList {
    pub fn statement_list(&self) -> AstChildren<Statement> {
        AstChildren::<Statement>::new(self.syntax())
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    syntax: SyntaxNode,
}

impl AstNode for Block {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::Block
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

impl Block {
    pub fn statement(&self) -> Option<AstStatementList> {
        self.syntax().children().find_map(AstStatementList::cast)
    }
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

#[derive(Debug, Clone)]
pub struct IdentifierDef {
    syntax: SyntaxNode,
}

impl AstNode for IdentifierDef {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::Identifier
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

impl IdentifierDef {
    pub fn name(&self) -> &SyntaxNode {
        self.syntax()
    }
}

#[derive(Debug, Clone)]
pub struct TemplateDef {
    syntax: SyntaxNode,
}

impl AstNode for TemplateDef {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::TemplateKw
    }

    fn cast(syntax: SyntaxNode) -> Option<Self>
    where
        Self: Sized,
    {
        if Self::can_cast(syntax.kind().into()) {
            return Some(TemplateDef { syntax });
        }
        None
    }

    fn syntax(&self) -> &SyntaxNode {
        &self.syntax
    }
}

impl TemplateDef {
    pub fn template_name(&self) -> Option<IdentifierDef> {
        self.syntax.children().find_map(IdentifierDef::cast)
    }
    pub fn func_body(&self) -> Option<Block> {
        self.syntax.children().find_map(Block::cast)
    }
}


#[derive(Debug)]
pub struct AstFunctionName {
    syntax: SyntaxNode
}

impl AstNode for AstFunctionName {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::FunctionName
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



#[derive(Debug, Clone)]
pub struct ASTFunction {
    syntax: SyntaxNode
}


impl AstNode for ASTFunction {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::FunctionDef
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

ast_node!(AstParameterList, TokenKind::ParameterList);

impl ASTFunction {
    pub fn body(&self) -> Option<Block> {
        self.syntax().children().find_map(Block::cast)
    } 

    pub fn function_name(&self) -> Option<AstFunctionName> {
        self.syntax().children().find_map(AstFunctionName::cast)
    }

    pub fn argument_list(&self) -> Option<AstParameterList> {
        self.syntax().children().find_map(AstParameterList::cast)
    }   
}

#[derive(Debug, Clone)]
pub struct CircomProgramAST {
    syntax: SyntaxNode,
}

impl AstNode for CircomProgramAST {
    fn can_cast(token_kind: TokenKind) -> bool {
        token_kind == TokenKind::CircomProgram
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

impl CircomProgramAST {
    pub fn pragma(&self) -> Option<PragmaDef> {
        self.syntax().children().find_map(PragmaDef::cast)
    }

    pub fn template_list(&self) -> Vec<TemplateDef> {
        self.syntax()
            .children()
            .filter_map(TemplateDef::cast)
            .collect()
    }
}
