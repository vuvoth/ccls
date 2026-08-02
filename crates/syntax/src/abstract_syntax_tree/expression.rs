//! Typed AST wrappers for expression nodes.
//!
//! The grammar reuses operator/keyword token kinds as node kinds for binary, prefix, and postfix
//! expressions (the rust-analyzer pattern), so an operand inside an `ArrayQuery`/`Call` node is the
//! raw inner node (`AstExpressionAtom`, another `AstArrayQuery`, …), not an `AstExpression`. Only
//! the outermost form is wrapped in a single `Expression` node by `expression::expression`. The
//! accessors below therefore expose the `Expression` children that are actually present, rather than
//! assuming a uniform operand shape.

use parser::token_kind::TokenKind::*;
use rowan::ast::support;

use crate::syntax_node::CircomLanguage;
use crate::syntax_node::SyntaxNode;
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;

use super::ast::AstIdentifier;

ast_node!(AstExpression, Expression);

impl AstExpression {
    /// The wrapped atom, when this expression is a bare literal/identifier (`5`, `0x1F`, `x`).
    /// Returns `None` for binary/prefix/postfix/ternary/parenthesized forms.
    pub fn atom(&self) -> Option<AstExpressionAtom> {
        support::child(self.syntax())
    }
}

ast_node!(AstExpressionAtom, ExpressionAtom);

impl AstExpressionAtom {
    /// The identifier this atom names, if any (number/hex atoms have no identifier wrapper).
    pub fn identifier(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstCall, Call);

impl AstCall {
    /// The argument expressions inside the parentheses. The callee (the expression being called)
    /// is not itself an `AstExpression`, so it is not included here.
    pub fn arguments(&self) -> Vec<AstExpression> {
        support::children(self.syntax()).collect()
    }
}

ast_node!(AstArrayQuery, ArrayQuery);

impl AstArrayQuery {
    /// The index expression inside the brackets. The indexed array is the raw inner node, not an
    /// `AstExpression`, and so is not exposed here.
    pub fn index(&self) -> Option<AstExpression> {
        support::child(self.syntax())
    }
}

ast_node!(AstTenaryConditional, TenaryConditional);

impl AstTenaryConditional {
    /// The three branches in order: `[0]` condition, `[1]` then-branch, `[2]` else-branch.
    pub fn branches(&self) -> Vec<AstExpression> {
        support::children(self.syntax()).collect()
    }
}
