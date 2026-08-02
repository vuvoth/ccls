//! Block and statement-list containers.

use parser::token_kind::TokenKind;
use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::node::{CircomLanguage, SyntaxNode};

use super::name::Named;

ast_node!(AstStatementList, StatementList);

impl AstStatementList {
    /// All direct children that cast to `N`. The grammar emits each statement as its concrete node
    /// kind (not a generic `Statement` node), so statements are queried by concrete type.
    pub fn find_children<N: AstNode<Language = CircomLanguage>>(&self) -> Vec<N> {
        support::children(self.syntax()).collect()
    }

    /// The first child of type `N` whose declared name equals `name`.
    pub fn find<N: Named>(&self, name: &str) -> Option<N> {
        self.find_children::<N>()
            .into_iter()
            .find(|n| n.identifier().is_some_and(|id| id.syntax().text() == name))
    }
}

ast_node!(AstBlock, Block);

impl AstBlock {
    pub fn statement_list(&self) -> Option<AstStatementList> {
        support::child(self.syntax())
    }
}
