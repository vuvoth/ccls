use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone, Copy)]
pub enum TokenKind {
    #[regex(r"//[^\n]*", logos::skip)]
    #[regex("[ \t]+", logos::skip)]
    #[regex("[\n]+", logos::skip)]
    #[error]
    Error = 0,

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
    EOF,
}

impl TokenKind {
    pub fn is_literal(self) -> bool {
        match self {
            Self::Number | Self::Identifier => true,
            _ => false,
        }
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
            Self::LBracket => Some(200),
            _ => None
        }
    }
    pub fn is_declaration_kw(self) -> bool {
        match self {
            Self::VarKw  | Self::ComponentKw | Self::SignalKw => true,
            _ => false
        }
    }
}
