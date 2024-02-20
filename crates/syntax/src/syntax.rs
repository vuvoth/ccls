use parser::input::Input;
use parser::output::{Child, Output};
use parser::parser::Parser;
use rowan::{GreenNode, GreenNodeBuilder};

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
                    let token_value = self.input.token_value(*token_id);
                    self.builder.start_node(token_kind.into());
                    self.builder.token(token_kind.into(), token_value);
                    self.builder.finish_node();
                }
                Child::Tree(child_tree) => self.build_rec(child_tree),
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
        let mut builder = SyntaxTreeBuilder::new(&input);
        let output = Parser::parsing(&input);
        builder.build(output);
        let green = builder.finish();
        SyntaxNode::new_root(green)
    }
}

#[cfg(test)]
mod tests {
    use super::SyntaxTreeBuilder;

    #[test]
    fn other_parser_test() {
        let source: String = r#"pragma circom 2.0.0;

        
        template Multiplier2 () {  
        
           // Declaration of signals.  
           signal input a;  
           signal input b;  
           signal output c;  
        
           // Constraints.  
           c <== a * b;  
        }
        "#
        .to_string();

        let syntax = SyntaxTreeBuilder::syntax_tree(&source);
        println!("{:#?}", syntax);
        // find token
    }
}
