use logos::Logos;
use serde::Serialize;

#[derive(Logos, Debug, PartialEq, Clone, Copy, Eq, PartialOrd, Ord, Hash, Serialize)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum TokenKind {
    #[error]
    Error = 0,
    #[regex(r"//[^\n]*")]
    CommentLine,
    #[token("/*")]
    CommentBlockOpen,
    #[token("*/")]
    CommentBlockClose,
    #[regex("[ \t]+")]
    WhiteSpace,
    #[regex("[\n]")]
    EndLine,
    #[token("pragma")]
    Pragma,
    #[token("circom")]
    Circom,
    #[regex("2.[0-9].[0-9]")]
    Version,
    #[regex("[0-9]+")]
    Number,
    #[regex("[$_]*[a-zA-Z][a-zA-Z0-9_$]*")]
    Identifier,
    #[regex(r#""[^"]*""#)]
    CircomString,
    #[token("template")]
    TemplateKw,
    #[token("function")]
    FunctionKw,
    #[token("component")]
    ComponentKw,
    #[token("main")]
    MainKw,
    #[token("public")]
    PublicKw,
    #[token("signal")]
    SignalKw,
    #[token("var")]
    VarKw,
    #[token("include")]
    IncludeKw,
    #[token("input")]
    InputKw,
    #[token("output")]
    OutputKw,
    #[token("log")]
    LogKw,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LCurly,
    #[token("}")]
    RCurly,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(";")]
    Semicolon,
    #[token(",")]
    Comma,
    #[token("=")]
    Assign,
    #[token("===")]
    EqualSignal,
    #[token("-->")]
    LAssignSignal,
    #[token("==>")]
    LAssignContraintSignal,
    #[token("<--")]
    RAssignSignal,
    #[token("<==")]
    RAssignConstraintSignal,
    #[token("+")]
    Add,
    #[token("-")]
    Sub,
    #[token("/")]
    Div,
    #[token("*")]
    Mul,
    #[token("!")]
    Not,
    #[token("~")]
    BitNot,
    #[token("**")]
    Power,
    #[token("\\")]
    IntDiv,
    #[token("%")]
    Mod,
    #[token("<<")]
    ShiftL,
    #[token(">>")]
    ShiftR,
    #[token("&")]
    BitAnd,
    #[token("|")]
    BitOr,
    #[token("^")]
    BitXor,
    #[token("==")]
    Equal,
    #[token("!=")]
    NotEqual,
    #[token("<")]
    LessThan,
    #[token(">")]
    GreaterThan,
    #[token("<=")]
    LessThanAndEqual,
    #[token(">=")]
    GreaterThanAndEqual,
    #[token("&&")]
    BoolAnd,
    #[token("||")]
    BoolOr,
    #[token("?")]
    MarkQuestion,
    #[token(":")]
    Colon,
    #[token(".")]
    Dot,
    #[token("if")]
    IfKw,
    #[token("else")]
    ElseKw,
    #[token("for")]
    ForKw,
    #[token("while")]
    WhileKw,
    #[token("return")]
    ReturnKw,
    #[token("assert")]
    AssertKw,
    ForLoop,
    AssignStatement,
    CircomProgram,
    SignalOfComponent,
    SignalHeader,
    Block,
    Tuple,
    TupleInit,
    Call,
    TenaryConditional,
    Condition,
    Expression,
    FunctionDef,
    Statement,
    StatementList,
    ComponentDecl,
    TemplateDef,
    TemplateName,
    FunctionName,
    ParameterList,
    SignalDecl,
    VarDecl,
    InputSignalDecl,
    OutputSignalDecl,
    ComponentCall,
    ComponentIdentifier,
    SignalIdentifier,
    ArrayQuery,
    ParserError,
    BlockComment,
    EOF,
    ROOT,
    __LAST,
}

impl From<u16> for TokenKind {
    #[inline]
    fn from(d: u16) -> TokenKind {
        assert!(d <= (TokenKind::__LAST as u16));
        unsafe { std::mem::transmute::<u16, TokenKind>(d) }
    }
}

impl From<rowan::SyntaxKind> for TokenKind {
    fn from(value: rowan::SyntaxKind) -> Self {
        match value {
            rowan::SyntaxKind(id) => TokenKind::from(id),
        }
    }
}

impl From<TokenKind> for u16 {
    #[inline]
    fn from(k: TokenKind) -> u16 {
        k as u16
    }
}

impl From<TokenKind> for rowan::SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        Self(kind as u16)
    }
}

impl TokenKind {
    pub fn is_literal(self) -> bool {
        matches!(self, Self::Number | Self::Identifier)
    }

    pub fn infix(self) -> Option<(u16, u16)> {
        match self {
            Self::BoolOr => Some((78, 79)),
            Self::BoolAnd => Some((80, 81)),
            Self::Equal
            | Self::NotEqual
            | Self::LessThan
            | Self::GreaterThan
            | Self::LessThanAndEqual
            | Self::GreaterThanAndEqual => Some((82, 83)),
            Self::BitOr => Some((84, 85)),
            Self::BitXor => Some((86, 87)),
            Self::BitAnd => Some((88, 89)),
            Self::ShiftL | Self::ShiftR => Some((90, 91)),
            Self::Add | Self::Sub => Some((92, 93)),
            Self::Mul | Self::Div | Self::IntDiv | Self::Mod => Some((94, 95)),
            Self::Power => Some((96, 97)),
            _ => None,
        }
    }

    pub fn prefix(self) -> Option<u16> {
        match self {
            Self::Sub => Some(100),
            Self::Not => Some(99),
            Self::BitNot => Some(98),
            _ => None,
        }
    }

    pub fn postfix(self) -> Option<u16> {
        match self {
            Self::Dot => Some(200),
            Self::LBracket => Some(201),
            _ => None,
        }
    }

    pub fn is_declaration_kw(self) -> bool {
        matches!(self, Self::VarKw | Self::ComponentKw | Self::SignalKw)
    }

    pub fn is_trivial(self) -> bool {
        matches!(
            self,
            Self::WhiteSpace | Self::EndLine | Self::CommentLine | Self::BlockComment | Self::Error
        )
    }
}
