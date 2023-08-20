use std::fmt;

use lsp_types::{Position, Range};

use crate::token_kind::TokenKind;

#[derive(Clone)]
pub struct Tree {
    pub(crate) kind: TokenKind,
    pub(crate) children: Vec<Child>,
}

// #[derive(Debug, Clone, Copy)]
// pub struct TokenRange {
//     pub start: u32,
//     pub end: u32,
// }

// impl TokenRange {
//     pub fn new(start: u32, end: u32) -> Self {
//         Self { start, end }
//     }
// }

// impl From<Range<usize>> for TokenRange {
//     fn from(value: Range<usize>) -> Self {
//         TokenRange { start: value.start, end: value.end}
//     }
// }

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub range: Range,
}

fn span_to_range(last_position: Position, text: &str) -> Range {
    let mut start = last_position;
    let mut end = last_position;
    let endline = '\n';
    let mut first_char = true;
    for c in text.chars() {
        if c.eq(&endline) {
            if end.line == 0 {
                end.line = 1;
            }
            end.line += 1;
            end.character = 0;
        } else {
            if end.line == 0 {
                end.line = 1;
                end.character = 0;
            } else {
                end.character += 1;
            }
        }

        if first_char {
            if c.eq(&endline) {
                if start.line == 0 {
                    start.line = 1;
                }
                start.line += 1;
                start.character = 0;
            } else {
                if start.line == 0 {
                    start.line = 1;
                    start.character = 0;
                } else {
                    start.character += 1;
                }
            }
            first_char = false;
        }
    }
    Range::new(start, end)
}

impl Token {
    pub fn new(kind: TokenKind, text: &str, span: logos::Span, last_position: Position) -> Self {
        Self {
            kind,
            text: text.to_string(),
            range: span_to_range(last_position, text),
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
                    if token.kind.is_travial() {
                        match token.kind {
                            TokenKind::WhiteSpace => {
                                format_to!(
                                    buf,
                                    "{indent} WhileSpace'{}'\n",
                                    token.text.replace("\n", "\\n"),
                                )
                            }
                            _ => {
                                unreachable!()
                            }
                        }
                    } else {
                        format_to!(buf, "{indent}  '{}'@{:?}\n", token.text, token.range)
                    }
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
