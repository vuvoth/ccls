//! Typed AST wrappers for statement nodes.
//!
//! `AstStatement`/`AstStatementList::statement_list()` are intentionally absent: the grammar does
//! not emit a `Statement` node (statement dispatch in `grammar::statement` is direct), so a typed
//! `Statement` wrapper would never cast. Statement nodes are instead queried by their concrete kind
//! via `AstStatementList::find_children::<N>()`.

use parser::token_kind::TokenKind;
use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::node::{CircomLanguage, SyntaxNode};

use super::block::AstBlock;
use super::expression::AstExpression;

ast_node!(AstIfStatement, IfStatement);

impl AstIfStatement {
    pub fn condition(&self) -> Option<AstExpression> {
        support::child(self.syntax())
    }
    /// The body when it is a `{ … }` block; `None` for a single-statement body.
    pub fn body_block(&self) -> Option<AstBlock> {
        support::child(self.syntax())
    }
}

ast_node!(AstWhileLoop, WhileLoop);

impl AstWhileLoop {
    pub fn condition(&self) -> Option<AstExpression> {
        support::child(self.syntax())
    }
    pub fn body_block(&self) -> Option<AstBlock> {
        support::child(self.syntax())
    }
}

ast_node!(AstForLoop, ForLoop);

impl AstForLoop {
    /// The loop's continuation condition (the middle clause of `for (init; cond; update)`).
    pub fn condition(&self) -> Option<AstExpression> {
        support::child(self.syntax())
    }
    pub fn body_block(&self) -> Option<AstBlock> {
        support::child(self.syntax())
    }
}

ast_node!(AstAssertStatement, AssertStatement);

impl AstAssertStatement {
    pub fn condition(&self) -> Option<AstExpression> {
        support::child(self.syntax())
    }
}

ast_node!(AstLogStatement, LogStatement);

impl AstLogStatement {
    /// The expression arguments passed to `log(…)`.
    pub fn arguments(&self) -> Vec<AstExpression> {
        support::children(self.syntax()).collect()
    }
}

ast_node!(AstReturnStatement, ReturnStatement);

impl AstReturnStatement {
    pub fn value(&self) -> Option<AstExpression> {
        support::child(self.syntax())
    }
}

ast_node!(AstAssignStatement, AssignStatement);

impl AstAssignStatement {
    /// The left- and (if present) right-hand expressions of the assignment: `[0]` lhs, `[1]` rhs.
    /// Postfix `++`/`--` substitutions yield a single lhs element.
    pub fn expressions(&self) -> Vec<AstExpression> {
        support::children(self.syntax()).collect()
    }
}
