use std::fmt;

use crate::token_kind::TokenKind;

#[derive(Clone)]
pub struct Tree {
    pub(crate) kind: TokenKind,
    pub(crate) children: Vec<Child>,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

impl Token {
    pub fn new(kind: TokenKind, text: &str) -> Self {
        Self {
            kind,
            text: text.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Child {
    Token(Token),
    Tree(Tree),
}

#[macro_export]
macro_rules! format_to {
    ($buf:expr) => ();
    ($buf:expr, $lit:literal $($arg:tt)*) => {
        { use ::std::fmt::Write as _; let _ = ::std::write!($buf, $lit $($arg)*); }
    };
}
impl Tree {
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

impl fmt::Debug for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = String::new();
        self.print(&mut buf, 0);
        write!(f, "{}", buf)
    }
}
