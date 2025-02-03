use parser::grammar::entry::Scope;
use parser::input::Input;
use parser::output::{Child, Output};
use parser::parser::Parser;
use parser::token_kind::TokenKind;
use rowan::{GreenNode, GreenNodeBuilder};

pub use rowan::{
    api::Preorder, Direction, NodeOrToken, SyntaxText, TextRange, TextSize, TokenAtOffset,
    WalkEvent,
};

use crate::syntax_node::SyntaxNode;

pub struct SyntaxTreeBuilder<'a> {
    builder: GreenNodeBuilder<'static>,
    input: &'a Input<'a>,
}

impl<'a> SyntaxTreeBuilder<'a> {
    pub fn new(input: &'a Input) -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
            input,
        }
    }
    pub fn build_rec(&mut self, tree: &Output) {
        self.builder.start_node(tree.kind().into());
        for child in tree.children() {
            match child {
                Child::Token(token_id) => {
                    let token_kind = self.input.kind_of(*token_id);
                    // TODO: return Error to replace .unwrap()
                    let token_value = self.input.token_value(*token_id).unwrap();
                    self.builder.start_node(token_kind.into());
                    self.builder.token(token_kind.into(), token_value);
                    self.builder.finish_node();
                }
                Child::Tree(child_tree) => self.build_rec(child_tree),
                Child::Error(error) => {
                    let token_kind = TokenKind::Error;
                    let token_value = error.as_str();

                    self.builder.start_node(token_kind.into());
                    self.builder.token(token_kind.into(), token_value);
                    self.builder.finish_node();
                }
            }
        }

        self.builder.finish_node();
    }

    pub fn build(&mut self, tree: Output) {
        self.build_rec(&tree);
    }

    pub fn finish(self) -> GreenNode {
        self.builder.finish()
    }

    pub fn syntax_tree(source: &str) -> SyntaxNode {
        let input = Input::new(source);

        let output = Parser::parsing(&input);

        let mut builder = SyntaxTreeBuilder::new(&input);
        builder.build(output);
        let green = builder.finish();
        SyntaxNode::new_root(green)
    }
}

pub fn syntax_node_from_source(source: &str, scope: Scope) -> SyntaxNode {
    let input = Input::new(&source);
    let output = Parser::parsing_with_scope(&input, scope);

    // output is a tree whose node is index of token, no content of token
    // convert output into green node
    let mut builder = SyntaxTreeBuilder::new(&input);
    builder.build(output);
    let green = builder.finish();

    // then cast green node into syntax node
    let syntax = SyntaxNode::new_root(green);

    syntax
}

#[cfg(test)]
mod tests {
    use crate::test_syntax;
    use parser::grammar::entry::Scope;

    #[test]
    fn pragma_happy_test() {
        test_syntax!("/src/test_files/happy/pragma.circom", Scope::Pragma);
    }

    #[test]
    fn template_happy_test() {
        // SOURCE & EXPECTED RESULT
        test_syntax!("/src/test_files/happy/template.circom", Scope::Template);
    }

    #[test]
    fn block_happy_test() {
        test_syntax!("/src/test_files/happy/block.circom", Scope::Block);
    }

    #[test]
    fn comment_happy_test() {
        test_syntax!(
            "/src/test_files/happy/block_comment.circom",
            Scope::CircomProgram
        );
        test_syntax!(
            "/src/test_files/happy/line_comment.circom",
            Scope::CircomProgram
        );
    }

    #[test]
    fn full_circom_program() {
        test_syntax!(
            "/src/test_files/happy/full_circom_program.circom",
            Scope::CircomProgram
        );
    }
}
