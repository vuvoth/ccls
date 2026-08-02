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
        let mut iter = events.into_iter();

        // A well-formed event stream starts with an `Open` (the root node). Anything else is
        // malformed; emit an empty tree instead of panicking.
        let root_kind = match iter.next() {
            Some(Event::Open { kind }) => kind,
            _ => return Output::empty(),
        };

        let mut root = Tree {
            kind: root_kind,
            children: Vec::new(),
        };
        let mut stack: Vec<Tree> = Vec::new();

        for event in iter {
            match event {
                Event::Open { kind } => stack.push(Tree {
                    kind,
                    children: Vec::new(),
                }),
                Event::Close => {
                    // Pop the finished subtree; if the stack is empty this is a stray `Close`,
                    // which we drop instead of panicking.
                    if let Some(tree) = stack.pop() {
                        attach_child(&mut stack, &mut root, Child::Tree(tree));
                    }
                }
                Event::TokenPosition(token) => {
                    attach_child(&mut stack, &mut root, Child::Token(token));
                }
                Event::ErrorReport(error) => {
                    attach_child(&mut stack, &mut root, Child::Error(error));
                }
            }
        }

        // Attach any leftover (unclosed) nodes to the root rather than dropping them.
        while let Some(tree) = stack.pop() {
            root.children.push(Child::Tree(tree));
        }

        root
    }
}

/// Append `child` to the innermost open node (top of `stack`), or to `root` when the stack is
/// empty. Splitting the borrow this way keeps tree construction panic-free.
fn attach_child(stack: &mut [Tree], root: &mut Tree, child: Child) {
    match stack.last_mut() {
        Some(node) => node.children.push(child),
        None => root.children.push(child),
    }
}

#[cfg(test)]
mod tests {
    use crate::event::Event;
    use crate::output::Output;
    use crate::token_kind::TokenKind;

    #[test]
    fn empty_events_yield_empty_tree() {
        let out: Output = Vec::<Event>::new().into();
        assert_eq!(out.kind(), TokenKind::ParserError);
        assert!(out.children().is_empty());
    }

    #[test]
    fn stray_leading_close_does_not_panic() {
        // A `Close` with no matching `Open` must not panic; it is dropped into an empty tree.
        let out: Output = vec![Event::Close].into();
        assert_eq!(out.kind(), TokenKind::ParserError);
        assert!(out.children().is_empty());
    }

    #[test]
    fn leading_token_without_open_does_not_panic() {
        let out: Output = vec![Event::TokenPosition(3)].into();
        assert_eq!(out.kind(), TokenKind::ParserError);
    }

    #[test]
    fn well_formed_stream_builds_a_tree() {
        // Open(root) Open(inner) Token Close Close  ->  root { inner { token } }
        let events = vec![
            Event::Open {
                kind: TokenKind::CircomProgram,
            },
            Event::Open {
                kind: TokenKind::Block,
            },
            Event::TokenPosition(0),
            Event::Close,
            Event::Close,
        ];
        let out: Output = events.into();
        assert_eq!(out.kind(), TokenKind::CircomProgram);
        assert_eq!(
            out.children().len(),
            1,
            "root should contain the inner block"
        );
    }
}
