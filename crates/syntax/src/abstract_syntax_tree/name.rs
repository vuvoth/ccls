//! Shared leaf primitives: the `Named` trait and the identifier / parameter-list nodes that every
//! other AST module builds on.

use parser::token_kind::TokenKind;
use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::syntax_node::{CircomLanguage, SyntaxNode};

/// A node that declares a nameable symbol and exposes its leaf identifier token — the `a` in
/// `signal input a;`, the `T` in `template T() {}`. Implementing it lets a declaration be looked up
/// generically via [`super::block::AstStatementList::find`] and compared by name without the caller
/// knowing the node's identifier-chain shape (`signal_identifier().name()` vs
/// `component_identifier().name()`).
pub trait Named: AstNode<Language = CircomLanguage> {
    fn identifier(&self) -> Option<AstIdentifier>;
}

ast_node!(AstIdentifier, Identifier);

ast_node!(AstComplexIdentifier, ComplexIdentifier);

impl AstComplexIdentifier {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstParameterList, ParameterList);

impl AstParameterList {
    pub fn parameters(&self) -> Vec<AstIdentifier> {
        support::children(self.syntax()).collect()
    }
}
