use crate::{event::Event, token_kind::TokenKind};

#[derive(Debug)]
pub enum Child {
    Token(usize), // position of token,
    Error(String),
    Tree(Tree),
}

#[derive(Debug)]
pub struct Tree {
    kind: TokenKind,
    children: Vec<Child>,
}

pub type Output = Tree;

impl Output {
    fn empty() -> Self {
        Tree {
            kind: TokenKind::ParserError,
            children: Vec::new(),
        }
    }

    pub fn kind(&self) -> TokenKind {
        self.kind
    }

    pub fn children(&self) -> &Vec<Child> {
        &self.children
    }
}

impl From<Vec<Event>> for Output {
    fn from(events: Vec<Event>) -> Self {
        let mut stack = Vec::new();
        if let Some((last, elements)) = events.split_last() {
            if !matches!(*last, Event::Close) {
                return Output::empty();
            }
            for event in elements {
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
                    Event::ErrorReport(error) => {
                        stack
                            .last_mut()
                            .unwrap()
                            .children
                            .push(Child::Error(error.clone()));
                    }
                }
            }
        }
        // TODO: Make it more safe
        stack.pop().unwrap()
    }
}
