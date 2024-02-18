use std::marker::PhantomData;


use crate::{
    syntax_node::{SyntaxNode, SyntaxNodeChildren, SyntaxToken},
    token_kind::{self, TokenKind, TokenKind::*},
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
            syntax: SyntaxNode,
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

ast_node!(AstStatement, Statement);
ast_node!(AstStatementList, StatementList);

impl AstStatementList {
    pub fn statement_list(&self) -> AstChildren<AstStatement> {
        AstChildren::<AstStatement>::new(self.syntax())
    }
}

ast_node!(AstBlock, Block);
impl AstBlock {
    pub fn statement(&self) -> Option<AstStatementList> {
        self.syntax().children().find_map(AstStatementList::cast)
    }
}

ast_node!(AstVersion, Version);
ast_node!(AstPragma, Pragma);

impl AstPragma {
    pub fn version(&self) -> Option<AstVersion> {
        self.syntax.children().find_map(AstVersion::cast)
    }
}
ast_node!(AstParameterList, TokenKind::ParameterList);

ast_node!(AstIdentifier, Identifier);

ast_node!(AstTemplateName, TemplateName);

ast_node!(AstTemplateDef, TemplateDef);

impl AstTemplateName {
    pub fn name(&self) -> Option<AstIdentifier> {
        self.syntax().children().find_map(AstIdentifier::cast)
    } 
}

impl AstTemplateDef {
    pub fn template_name(&self) -> Option<AstTemplateName> {
        self.syntax.children().find_map(AstTemplateName::cast)
    }
    pub fn func_body(&self) -> Option<AstBlock> {
        self.syntax.children().find_map(AstBlock::cast)
    }
    pub fn parameter_list(&self) -> Option<AstParameterList> {
        self.syntax().children().find_map(AstParameterList::cast)
    }
}

ast_node!(AstFunctionName, FunctionName);

ast_node!(AstFunctionDef, FunctionDef);

impl AstFunctionDef {
    pub fn body(&self) -> Option<AstBlock> {
        self.syntax().children().find_map(AstBlock::cast)
    }

    pub fn function_name(&self) -> Option<AstFunctionName> {
        self.syntax().children().find_map(AstFunctionName::cast)
    }

    pub fn argument_list(&self) -> Option<AstParameterList> {
        self.syntax().children().find_map(AstParameterList::cast)
    }
}

ast_node!(AstCircomProgram, CircomProgram);

impl AstCircomProgram {
    pub fn pragma(&self) -> Option<AstPragma> {
        self.syntax().children().find_map(AstPragma::cast)
    }

    pub fn template_list(&self) -> Vec<AstTemplateDef> {
        self.syntax()
            .children()
            .filter_map(AstTemplateDef::cast)
            .collect()
    }

    pub fn function_list(&self) -> Vec<AstFunctionDef> {
        self.syntax()
            .children()
            .filter_map(AstFunctionDef::cast)
            .collect()
    }
}
