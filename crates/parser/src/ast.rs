/// This code borrow from rust analyzer
/// Thank you for amazing implementation.
///
use std::marker::PhantomData;

pub use rowan::ast::{support, AstChildren, AstNode};
use crate::syntax_node::CircomLanguage;

use crate::{
    syntax_node::{SyntaxNode, SyntaxNodeChildren, SyntaxToken},
    token_kind::{TokenKind, TokenKind::*},
};




macro_rules! ast_node {
    ($ast_name: ident, $kind: expr) => {
        #[derive(Debug, Clone)]
        pub struct $ast_name {
            syntax: SyntaxNode,
        }
        impl AstNode for $ast_name {
            type Language = CircomLanguage;
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

ast_node!(AstStatement, Statement);
ast_node!(AstStatementList, StatementList);

impl AstStatementList {
    pub fn statement_list(&self) -> AstChildren<AstStatement> {
        support::children(self.syntax())
    }
}

ast_node!(AstBlock, Block);
impl AstBlock {
    pub fn statement(&self) -> Option<AstStatementList> {
        support::child::<AstStatementList>(self.syntax())
    }
}

ast_node!(AstVersion, Version);
ast_node!(AstPragma, Pragma);

impl AstPragma {
    pub fn version(&self) -> Option<AstVersion> {
        support::child(self.syntax())
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
