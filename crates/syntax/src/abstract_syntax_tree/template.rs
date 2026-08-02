use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::syntax_node::CircomLanguage;
use crate::syntax_node::SyntaxNode;
use parser::token_kind::TokenKind;

use super::ast::{AstBlock, AstIdentifier, AstParameterList, AstStatementList, Named};

ast_node!(AstTemplateName, TemplateName);

impl AstTemplateName {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
impl Named for AstTemplateName {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.name()
    }
}

ast_node!(AstTemplateDef, TemplateDef);

impl AstTemplateDef {
    pub fn name(&self) -> Option<AstTemplateName> {
        support::child(self.syntax())
    }
    pub fn body(&self) -> Option<AstBlock> {
        support::child(self.syntax())
    }
    pub fn parameter_list(&self) -> Option<AstParameterList> {
        support::child(self.syntax())
    }
    pub fn statements(&self) -> Option<AstStatementList> {
        self.body().and_then(|b| b.statement_list())
    }
}
impl Named for AstTemplateDef {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.name().and_then(|n| n.name())
    }
}
