use rowan::{GreenNode, GreenNodeBuilder};

use crate::event;
use crate::{event::Event, input::Input};

use crate::token_kind::TokenKind::{self, *};

pub struct CircomParser<'a> {
    builder: GreenNodeBuilder<'static>,
    input: &'a Input<'a>,
}

impl From<TokenKind> for rowan::SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        Self(kind as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CircomLang {}

impl rowan::Language for CircomLang {
    type Kind = TokenKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        assert!(raw.0 <= ROOT as u16);
        unsafe { std::mem::transmute::<u16, TokenKind>(raw.0) }
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
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
                Child::Token(token_id) => self.builder.token(
                    self.input.kind_of(token_id).into(),
                    self.input.token_value(token_id),
                ),
                Child::Tree(child_tree) => self.build_rec(child_tree),
            }
        }

        self.builder.finish_node();
    }

    pub fn build(&mut self, tree: Tree) {
        self.builder.start_node(ROOT.into());
        self.build_rec(tree);
        self.builder.finish_node();
    }

    pub fn finish(self) -> GreenNode {
        self.builder.finish()
    }
}
