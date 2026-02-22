//! Navigation utilities
//!
//! Finding tokens and symbols at positions in the syntax tree.

use lsp_types::Position;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::Program;
use syntax::syntax_node::SyntaxToken;

use crate::database::FileDB;

use super::is_symbol_token;

/// Find the token at a given LSP position
pub fn token_at(file: &FileDB, program: &Program, pos: Position) -> Option<SyntaxToken> {
    let offset = file.offset(pos);
    let tokens = program.syntax().token_at_offset(offset);

    // Try exact match first
    if let Some(token) = tokens.clone().find(|t| is_symbol_token(t.kind())) {
        return Some(token);
    }

    // Fall back to left-biased (for cursor at end of token)
    tokens.left_biased().filter(|t| is_symbol_token(t.kind()))
}

/// Create a Location from a SyntaxNode
pub fn location(file: &FileDB, node: &syntax::syntax_node::SyntaxNode) -> lsp_types::Location {
    lsp_types::Location::new(file.url.clone(), file.range(node))
}
