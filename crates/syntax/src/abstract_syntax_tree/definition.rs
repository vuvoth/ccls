//! Top-level definitions: `template`, `function`, and `bus`. These share the parser's
//! `definition_body` grammar (`name (params)? block`), so their typed accessors are identical in
//! shape and live together here.

use parser::token_kind::TokenKind;
use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::syntax_node::{CircomLanguage, SyntaxNode};

use super::block::{AstBlock, AstStatementList};
use super::name::{AstIdentifier, AstParameterList, Named};

// --- template ----------------------------------------------------------------

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

// --- function ----------------------------------------------------------------

ast_node!(AstFunctionName, FunctionName);

impl AstFunctionName {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
impl Named for AstFunctionName {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.name()
    }
}

ast_node!(AstFunctionDef, FunctionDef);

impl AstFunctionDef {
    pub fn name(&self) -> Option<AstFunctionName> {
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
impl Named for AstFunctionDef {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.name().and_then(|n| n.name())
    }
}

// --- bus ---------------------------------------------------------------------

ast_node!(AstBusName, BusName);

impl AstBusName {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
impl Named for AstBusName {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.name()
    }
}

ast_node!(AstBusDef, BusDef);

impl AstBusDef {
    pub fn name(&self) -> Option<AstBusName> {
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
impl Named for AstBusDef {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.name().and_then(|n| n.name())
    }
}
