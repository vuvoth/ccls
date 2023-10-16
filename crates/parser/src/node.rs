use std::fmt;

use lsp_types::{Position, Range};

use crate::token_kind::TokenKind;

#[derive(Clone)]
pub struct Tree {
    pub kind: TokenKind,
    pub children: Vec<Child>,
}

impl Tree {
    pub fn get_range(self) -> Range{
        let mut right = Position {line: 0, character: 0};
        let mut left = Position {line: 10000000, character: 100000}; 
        for child in self.children {
            match  child{
                Child::Token(token) => {
                    if equal_or_greater(left, token.range.start) {
                        left = token.range.start;
                    }
                    if equal_or_greater(token.range.end, right) {
                        right = token.range.end
                    }
                },
                Child::Tree(tree) => {
                    let sub_range = tree.get_range();
                    if equal_or_greater(left, sub_range.start) {
                        left = sub_range.start;
                    }
                    if equal_or_greater(sub_range.end, right) {
                        right = sub_range.end;
                    } 
                }
            }
        }

        return Range{start: left, end: right};
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub range: Range,
}

fn span_to_range(last_position: Position, span: logos::Span, text: &str) -> Range {
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

fn equal_or_greater(l: Position, r: Position) -> bool {
    if l.line == r.line {
        return l.character >= r.character;
    }
    return  l.line > r.line;
}

impl Token {
    pub fn new(kind: TokenKind, text: &str, span: logos::Span, last_position: Position) -> Self {
        let range = span_to_range(last_position, span, text);
        Self {
            kind,
            text: text.to_string(),
            range,
        }
    }

    pub fn is_wrap(self, pos: Position) -> bool {
        let ran = self.range;
        return equal_or_greater(pos, ran.start) && equal_or_greater(ran.end, pos);
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
                            TokenKind::WhiteSpace | TokenKind::EndLine => {
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
        // assert!(buf.ends_with('\n'));
    }
}

impl fmt::Debug for Tree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = String::new();
        self.print(&mut buf, 0);
        write!(f, "{}", buf)
    }
}

