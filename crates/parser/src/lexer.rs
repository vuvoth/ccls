use logos::Logos;
use std::ops::Range;

pub type Span = Range<usize>;

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum LexerError {
    #[default]
    Invalid,
    UnterminatedString,
    UnterminatedBlockComment,
}

impl LexerError {
    pub fn into_diagnostic(self, span: Span) -> Diagnostic {
        let message = match self {
            Self::Invalid => "invalid token".to_string(),
            Self::UnterminatedString => "unterminated string literal".to_string(),
            Self::UnterminatedBlockComment => "unterminated block comment".to_string(),
        };
        Diagnostic { message, span }
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Logos, Debug, PartialEq, Copy, Clone)]
pub enum Token {
    #[error]
    Error,

    EOF,

    #[regex(r"[ \t\n\f\r]+")]
    Whitespace,

    #[token("template")]
    Template,
    #[token("function")]
    Function,
    #[token("signal")]
    Signal,
    #[token("input")]
    Input,
    #[token("output")]
    Output,
    #[token("var")]
    Var,
    #[token("component")]
    Component,
    #[token("pragma")]
    Pragma,
    #[token("circom")]
    Circom,
    #[token("include")]
    Include,
    #[token("main")]
    Main,
    #[token("public")]
    Public,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("for")]
    For,
    #[token("while")]
    While,
    #[token("return")]
    Return,
    #[token("log")]
    Log,
    #[token("assert")]
    Assert,

    #[token("(")]
    LPar,
    #[token(")")]
    RPar,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBrack,
    #[token("]")]
    RBrack,
    #[token(";")]
    Semi,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("?")]
    Question,
    #[token(":")]
    Colon,

    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("**")]
    StarStar,
    #[token("/")]
    Slash,
    #[token("\\")]
    IntDiv,
    #[token("%")]
    Percent,

    #[token("&")]
    Ampersand,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("<<")]
    LtLt,
    #[token(">>")]
    GtGt,

    #[token("&&")]
    AmpAmp,
    #[token("||")]
    PipePipe,
    #[token("!")]
    Bang,

    #[token("==")]
    EqEq,
    #[token("!=")]
    BangEq,
    #[token("<")]
    LessThan,
    #[token(">")]
    GreaterThan,
    #[token("<=")]
    LessThanEq,
    #[token(">=")]
    GreaterThanEq,

    #[token("=")]
    Eq,
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("**=")]
    StarStarEq,
    #[token("/=")]
    SlashEq,
    #[token("\\=")]
    IntDivEq,
    #[token("%=")]
    PercentEq,
    #[token("&=")]
    AmpEq,
    #[token("|=")]
    PipeEq,
    #[token("^=")]
    CaretEq,
    #[token("<<=")]
    LtLtEq,
    #[token(">>=")]
    GtGtEq,

    #[token("++")]
    PlusPlus,
    #[token("--")]
    MinusMinus,

    #[token("===")]
    EqEqEq,
    #[token("-->")]
    ArrowR,
    #[token("==>")]
    ConstrainR,
    #[token("<--")]
    ArrowL,
    #[token("<==")]
    ConstrainL,

    #[regex(r"[$_]*[a-zA-Z][a-zA-Z0-9_$]*")]
    Identifier,

    #[regex(r"[0-9]+")]
    Number,

    #[regex(r"[0-9]+\.[0-9]+\.[0-9]+")]
    Version,

    #[regex(r"//[^\r\n]*")]
    CommentLine,

    #[regex(r"/\*([^*]|\*[^/])*\*/")]
    CommentBlock,

    #[token("\"")]
    String,
}

pub fn tokenize(source: &str, diags: &mut Vec<Diagnostic>) -> (Vec<Token>, Vec<Span>) {
    let mut tokens = vec![];
    let mut spans = vec![];
    let mut i = 0;

    while i < source.len() {
        let remaining = &source[i..];

        if remaining.starts_with("/*") {
            let start = i;
            i += 2;
            while i < source.len() && !source[i..].starts_with("*/") {
                i += source[i..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(1);
            }
            if i >= source.len() {
                diags.push(LexerError::UnterminatedBlockComment.into_diagnostic(start..i));
            } else {
                i += 2;
            }
            tokens.push(Token::CommentBlock);
            spans.push(start..i);
            continue;
        }

        if remaining.starts_with('"') {
            let start = i;
            i += 1;
            let mut escaped = false;
            while i < source.len() {
                let ch = source[i..].chars().next().unwrap();
                if escaped {
                    escaped = false;
                    i += ch.len_utf8();
                } else if ch == '\\' {
                    escaped = true;
                    i += 1;
                } else if ch == '"' {
                    i += 1;
                    break;
                } else {
                    i += ch.len_utf8();
                }
            }
            if !source[start..]
                .chars()
                .last()
                .map(|c| c == '"')
                .unwrap_or(false)
                || source[start..].len() < 2
            {
                diags.push(LexerError::UnterminatedString.into_diagnostic(start..i));
            }
            tokens.push(Token::String);
            spans.push(start..i);
            continue;
        }

        let slice = &source[i..];
        let mut lexer = Token::lexer(slice);

        if let Some(token) = lexer.next() {
            let span = lexer.span();
            let abs_span = (i + span.start)..(i + span.end);
            if token != Token::Error {
                tokens.push(token);
            } else {
                diags.push(LexerError::Invalid.into_diagnostic(abs_span.clone()));
                tokens.push(Token::Error);
            }
            spans.push(abs_span.clone());
            i = abs_span.end;
        } else if i < source.len() {
            tokens.push(Token::Error);
            spans.push(i..i + 1);
            i += 1;
        }
    }

    (tokens, spans)
}
