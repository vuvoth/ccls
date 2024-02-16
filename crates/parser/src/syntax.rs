use rowan::{GreenNode, GreenNodeBuilder};

use crate::{event::Event, input::Input};

use crate::token_kind::TokenKind::{self, *};

pub struct CircomParser<'a> {
    builder: GreenNodeBuilder<'static>,
    input: &'a Input<'a>,
}

#[derive(Debug)]
pub enum Child {
    Token(usize), // position of token,
    Tree(Tree),
}

#[derive(Debug)]
pub struct Tree {
    kind: TokenKind,
    children: Vec<Child>,
}

// build green node from event and input
pub fn covert_to_tree_format(events: &mut Vec<Event>) -> Tree {
    let mut stack = Vec::new();

    assert!(matches!(events.pop(), Some(Event::Close)));

    for event in events {
        match event {
            Event::Open { kind } => {
                stack.push(Tree {
                    kind: *kind,
                    children: Vec::new(),
                });
            }
            Event::Close => {
                let tree = stack.pop().unwrap();

                stack.last_mut().unwrap().children.push(Child::Tree(tree));
            }
            Event::TokenPosition(token) => {
                stack
                    .last_mut()
                    .unwrap()
                    .children
                    .push(Child::Token(*token));
            }
        }
    }

    let tree = stack.pop().unwrap();

    tree
}

impl<'a> CircomParser<'a> {
    pub fn new(input: &'a Input) -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
            input,
        }
    }
    pub fn build_rec(&mut self, tree: Tree) {
        self.builder.start_node(tree.kind.into());
        for child in tree.children {
            match child {
                Child::Token(token_id) => {
                    let token_kind = self.input.kind_of(token_id);
                    let token_value = self.input.token_value(token_id);
                    self.builder.start_node(token_kind.into());
                    self.builder.token(token_kind.into(), token_value);
                    self.builder.finish_node();
                }
                Child::Tree(child_tree) => self.build_rec(child_tree),
            }
        }

        self.builder.finish_node();
    }

    pub fn build(&mut self, tree: Tree) {
        self.build_rec(tree);
    }

    pub fn finish(self) -> GreenNode {
        self.builder.finish()
    }
}

#[cfg(test)]
mod tests {
    use rowan::SyntaxNode;

    use crate::{parser::Parser, syntax_node::CircomLang};

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

        let green_node = Parser::parse_circom(&source);
        let syntax_node = SyntaxNode::<CircomLang>::new_root(green_node.clone());

        syntax_node.text();
        // find token
    }
}
