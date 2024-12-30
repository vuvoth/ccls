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
    // a + 10 --> a and 10 are literals
    pub fn is_literal(self) -> bool {
        matches!(self, Self::Number | Self::Identifier)
    }

    // these tokens have the lowest priority
    // <identifier1> infix_operator <identifier2>
    // eg: a + b --> + is an infix token
    pub fn infix(self) -> Option<(u16, u16)> {
        match self {
            // arithmetic operators
            Self::Power => Some((99, 100)),
            Self::Mul | Self::Div | Self::IntDiv | Self::Mod => Some((94, 95)),
            Self::Add | Self::Sub => Some((89, 90)),
            // shift bitwise operators
            Self::ShiftL | Self::ShiftR => Some((84, 85)),
            // relational operators
            Self::LessThan
            | Self::GreaterThan
            | Self::LessThanAndEqual
            | Self::GreaterThanAndEqual => Some((79, 80)),
            Self::Equal
            | Self::NotEqual => Some((74, 75)),
            // other bitwise operators
            Self::BitAnd => Some((69, 70)),
            Self::BitXor => Some((64, 65)), // exclusive or
            Self::BitOr => Some((59, 60)),
            // boolean operators
            Self::BoolAnd => Some((54, 55)),
            Self::BoolOr => Some((49, 50)),
            // ----------
            // TODO: how about conditional operation ( ? : )
            // associativity: right to left [ a ? b : c --> ??? ] 
            // ----------
            // associativity: right to left [ a = b = c --> a = (b = c) ] 
            // assignment operators
            Self::Assign
            // bitwise asignment operators
            | Self::BitOrAssign
            | Self::BitXorAssign
            | Self::BitAndAssign
            | Self::ShiftLAssign
            | Self::ShiftRAssign
            // arithmetic asignament operators
            | Self::AddAssign
            | Self::SubAssign
            | Self::MulAssign
            | Self::DivAssign
            | Self::IntDivAssign
            | Self::ModAssign
            | Self::PowerAssign => Some((44, 45)),
            // TODO: how about comma (expression separator)
            Self::Comma => Some((39, 40)),
            // not an infix operator
            _ => None,
        }
    }

    // priority: post > pre > in
    // associativity: right to left [ --!a --> --(!a) ]
    // prefix_operator <literal>
    // eg: -10, !a, ++a, --a
    pub fn prefix(self) -> Option<u16> {
        match self {
            Self::UnitDec | Self::UnitInc | Self::Sub | Self::Add | Self::Not | Self::BitNot => {
                Some(200)
            }

            _ => None,
        }
    }

    // these tokens have the highest priority
    // <literal> postfix_operator
    // eg: a[10], b++, c.att1
    pub fn postfix(self) -> Option<u16> {
        match self {
            Self::LParen // function call
            | Self::LBracket // array subscript
            | Self::Dot // attribute access
            | Self::UnitDec | Self::UnitInc => Some(300),

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
