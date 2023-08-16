use std::fmt;

use crate::token_kind::TokenKind;

pub struct Tree<'a> {
    pub(crate) kind: TokenKind,
    pub(crate) children: Vec<Child<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct Token<'a> {
    pub kind: TokenKind,
    pub text: &'a str,
}

impl<'a> Token<'a> {
    pub fn new(kind: TokenKind, text: &'a str) -> Self {
        Self { kind, text }
    }
}

#[derive(Debug)]
pub enum Child<'a> {
    Token(Token<'a>),
    Tree(Tree<'a>),
}

#[macro_export]
macro_rules! format_to {
    ($buf:expr) => ();
    ($buf:expr, $lit:literal $($arg:tt)*) => {
        { use ::std::fmt::Write as _; let _ = ::std::write!($buf, $lit $($arg)*); }
    };
}
impl Tree<'_> {
    pub fn print(&self, buf: &mut String, level: usize) {
        let indent = "  ".repeat(level);
        format_to!(buf, "{indent}{:?}\n", self.kind);
        for child in &self.children {
            match child {
                Child::Token(token) => {
                    format_to!(buf, "{indent}  '{}'\n", token.text)
                }
                Child::Tree(tree) => tree.print(buf, level + 1),
            }
        }
        assert!(buf.ends_with('\n'));
    }
}

impl fmt::Debug for Tree<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = String::new();
        self.print(&mut buf, 0);
        write!(f, "{}", buf)
    }
}
