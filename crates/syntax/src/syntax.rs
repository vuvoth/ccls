use parser::{Cst, Node, NodeRef, Parser};
use rowan::{GreenNode, GreenNodeBuilder};

pub use rowan::{
    api::Preorder, Direction, NodeOrToken, SyntaxText, TextRange, TextSize, TokenAtOffset,
    WalkEvent,
};

use crate::syntax_node::{SyntaxKind, SyntaxNode};

#[derive(Debug)]
pub enum SyntaxError {
    InvalidTree(String),
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyntaxError::InvalidTree(msg) => write!(f, "Malformed syntax tree: {}", msg),
        }
    }
}

impl std::error::Error for SyntaxError {}

pub struct SyntaxTreeBuilder;

impl SyntaxTreeBuilder {
    pub fn build(cst: &Cst, source: &str) -> GreenNode {
        let mut builder = GreenNodeBuilder::new();
        let root_ref = NodeRef::ROOT;
        Self::convert_node(&mut builder, cst, root_ref, source);
        builder.finish()
    }

    fn convert_node(builder: &mut GreenNodeBuilder, cst: &Cst, node_ref: NodeRef, source: &str) {
        match cst.get(node_ref) {
            Node::Rule(rule, _) => {
                builder.start_node(SyntaxKind::from_rule(rule).into());
                for child_ref in cst.children(node_ref) {
                    Self::convert_node(builder, cst, child_ref, source);
                }
                builder.finish_node();
            }
            Node::Token(token, _span_idx) => {
                if let Some((text, _span)) = cst.match_token(node_ref, token) {
                    builder.token(SyntaxKind::from_token(token).into(), text);
                }
            }
        }
    }

    pub fn syntax_tree(source: &str) -> SyntaxNode {
        let mut diags = Vec::new();
        let parser = Parser::new(source, &mut diags);
        let cst = parser.parse(&mut diags);
        let green = Self::build(&cst, source);
        SyntaxNode::new_root(green)
    }
}

pub fn syntax_node_from_source(source: &str) -> SyntaxNode {
    SyntaxTreeBuilder::syntax_tree(source)
}

#[cfg(test)]
mod test_utils;

#[cfg(test)]
mod tests {
    use super::SyntaxTreeBuilder;
    use crate::syntax::test_utils::view_ast;
    use crate::test_syntax;

    #[test]
    fn test_syntax_tree_creation() {
        let source = "template Test() { signal input in; }";
        let ast = SyntaxTreeBuilder::syntax_tree(source);
        assert!(ast.children().next().is_some());
    }

    #[test]
    fn pragma_happy_test() {
        test_syntax!("tests/fixtures/syntax/happy/pragma.circom");
    }

    #[test]
    fn template_happy_test() {
        test_syntax!("tests/fixtures/syntax/happy/template.circom");
    }

    #[test]
    fn block_happy_test() {
        test_syntax!("tests/fixtures/syntax/happy/block.circom");
    }

    #[test]
    fn statements_happy_test() {
        test_syntax!("tests/fixtures/syntax/happy/statements.circom");
    }

    #[test]
    fn comment_happy_test() {
        test_syntax!("tests/fixtures/syntax/happy/block_comment.circom");
        test_syntax!("tests/fixtures/syntax/happy/line_comment.circom");
    }

    #[test]
    fn full_circom_program() {
        test_syntax!("tests/fixtures/syntax/happy/full_circom_program.circom");
    }
}
