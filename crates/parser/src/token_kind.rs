use logos::Logos;
use serde::Serialize;

#[derive(Logos, Debug, PartialEq, Clone, Copy, Eq, PartialOrd, Ord, Hash, Serialize)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum TokenKind {
    // Error
    #[error]
    Error = 0,
    // Comments
    #[regex(r"//[^\n]*")]
    CommentLine,
    #[token("/*")]
    CommentBlockOpen,
    #[token("*/")]
    CommentBlockClose,
    // Trivial
    #[regex("[ \t]+")]
    WhiteSpace,
    #[regex("[\n]")]
    EndLine,
    // Circom
    #[token("pragma")]
    Pragma,
    #[token("circom")]
    Circom,
    #[regex("2.[0-9].[0-9]")]
    Version,
    // Literals
    #[regex("[0-9]+")]
    Number,
    #[regex("[$_]*[a-zA-Z][a-zA-Z0-9_$]*")]
    Identifier,
    #[regex(r#""[^"]*""#)]
    CircomString,
    #[token("template")]
    TemplateKw,
    // Brackets
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
    // Punctuation
    #[token(";")]
    Semicolon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    // Boolean operators
    #[token("&&")]
    BoolAnd,
    #[token("||")]
    BoolOr,
    #[token("!")]
    Not,
    // Relational operators
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
    // Arithmetic operators
    #[token("+")]
    Add,
    #[token("-")]
    Sub,
    #[token("*")]
    Mul,
    #[token("**")]
    Power,
    #[token("/")]
    Div,
    #[token("\\")]
    IntDiv,
    #[token("%")]
    Mod,
    // Combined arithmetic assignment
    #[token("+=")]
    AddAssign,
    #[token("-=")]
    SubAssign,
    #[token("*=")]
    MulAssign,
    #[token("**=")]
    PowerAssign,
    #[token("/=")]
    DivAssign,
    #[token(r"\=")]
    IntDivAssign,
    #[token("%=")]
    ModAssign,
    #[token("++")]
    UnitInc,
    #[token("--")]
    UnitDec,
    // Bitwise operators
    #[token("&")]
    BitAnd,
    #[token("|")]
    BitOr,
    #[token("~")]
    BitNot,
    #[token("^")]
    BitXor,
    #[token(">>")]
    ShiftR,
    #[token("<<")]
    ShiftL,
    // Combined bitwise assignments
    #[token("&=")]
    BitAndAssign,
    #[token("|=")]
    BitOrAssign,
    #[token("~=")]
    BitNotAssign,
    #[token("^=")]
    BitXorAssign,
    #[token(">>=")]
    ShiftRAssign,
    #[token("<<=")]
    ShiftLAssign,
    // Assign
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
    // Conditional expressions
    #[token("?")]
    MarkQuestion,
    #[token(":")]
    Colon,
    // Keywords
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
    // Statement keywords
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
    // Complex token kind
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
            // TODO: review
            Self::AddAssign | Self::SubAssign => Some((98, 99)),
            Self::MulAssign | Self::DivAssign | Self::IntDivAssign | Self::ModAssign => {
                Some((100, 101))
            }
            Self::PowerAssign => Some((102, 103)),
            _ => None,
        }
    }

    pub fn prefix(self) -> Option<u16> {
        match self {
            // TODO: review UnitDec, UnitInc
            Self::UnitDec | Self::UnitInc => Some(101),
            Self::Sub => Some(100),
            Self::Not => Some(99),
            Self::BitNot => Some(98),
            _ => None,
        }
    }

    pub fn postfix(self) -> Option<u16> {
        match self {
            // TODO: review UnitDec, UnitInc
            Self::UnitDec | Self::UnitInc => Some(202),
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
