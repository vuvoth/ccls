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
    #[regex(r"//[^\r\n]*")]
    CommentLine,
    #[token("/*")]
    CommentBlockOpen,
    #[token("*/")]
    CommentBlockClose,
    // Trivial
    #[regex("[ \t]+")]
    WhiteSpace,
    #[regex(r"\r?\n")]
    EndLine,
    // Pragma
    Pragma,
    #[token("pragma")]
    PragmaKw,
    #[token("circom")]
    Circom,
    // Version used by `pragma circom <version>`; grammar form is N.N.N (e.g. 2.0.0, 2.1.12).
    // Each component is one or more digits. Declared before `Number` so the longest match
    // (`2.0.0`) wins over a bare numeric literal (`2`).
    #[regex(r"[0-9]+\.[0-9]+\.[0-9]+")]
    Version,
    // Literals
    // Hexadecimal literal (grammar terminal `0x[0-9A-Fa-f]*` — ZERO digits allowed, so `0x` alone
    // is a valid HexNumber). Declared before `Number` so the longest match wins: `0x1F` lexes as a
    // single HexNumber, not Number `0` + Identifier `x1F`.
    #[regex(r"0x[0-9A-Fa-f]*")]
    HexNumber,
    #[regex("[0-9]+")]
    Number,
    #[regex("[$_]*[a-zA-Z][a-zA-Z0-9_$]*")]
    Identifier,
    // String literal (grammar terminal `"[^"\n]*"` — single-line; a newline terminates it). The
    // `[^"\n]*` body excludes both `"` and newline, matching the official lalrpop regex exactly.
    #[regex(r#""[^"\n]*""#)]
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
    #[token(";")]
    Semicolon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    // Anonymous placeholder (grammar Expression0: `"_"`). A lone `_` is not an Identifier (the
    // ident regex requires a letter), so it needs its own terminal.
    #[token("_")]
    Underscore,
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
    // Template modifiers / definition keywords (grammar ParseDefinition). logos longest-match
    // disambiguates `custom` from `custom_templates` (the longer token wins when both match).
    #[token("custom")]
    CustomKw,
    #[token("custom_templates")]
    CustomTemplatesKw,
    #[token("extern_c")]
    ExternCKw,
    #[token("parallel")]
    ParallelKw,
    #[token("bus")]
    BusKw,
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
    // Statements
    IfStatement,
    AssertStatement,
    LogStatement,
    ReturnStatement,
    AssignStatement,
    ExpressionStatement,
    ForLoop,
    WhileLoop,
    // Program
    CircomProgram,
    // Include statement node (distinct from the IncludeKw keyword token)
    Include,
    // Function
    FunctionDef,
    FunctionName,
    // Template
    TemplateDef,
    TemplateName,
    // Bus definition (grammar ParseDefinition: `bus Name (params)? block`)
    BusDef,
    BusName,
    // ComplexIdentifier, which will replace:
    // ___ SignalIdentifier,
    // ___ VarIdentifier,
    // ___ ComponentIdentifier,
    ComplexIdentifier,
    // Signal
    SignalDecl,
    InputSignalDecl,
    OutputSignalDecl,
    SignalHeader,
    // Variable
    VarDecl,
    // Component
    ComponentDecl,
    ComponentCall,
    // Expression
    ExpressionAtom,
    Expression,
    // ≥2-element parenthesized expression literal `(a, b, …)` (grammar Expression1 tuple form)
    TupleExpr,
    // `[a, b, …]` inline array literal (grammar Expression1 inline-array form)
    InlineArray,
    // Complex token kind
    MainComponent,
    Block,
    ParameterList,
    Call,
    TernaryConditional,
    StatementList,
    ArrayQuery,
    ParserError,
    BlockComment,
    EOF,
    __LAST,
}

/// Pratt binding powers (higher binds tighter). These mirror the precedence tiers of the
/// official circom grammar (`lang.lalrpop`, `Expression0`..`Expression13`):
///
/// ```text
/// postfix () [] .  >  prefix ! ~ -  >  **  >  * / \ %  >  + -  >  << >>
///   >  &  >  ^  >  |  >  == != < > <= >=  >  &&  >  ||
/// ```
///
/// All core circom infix operators are **left-associative**, so `infix()` returns
/// `(lbp, lbp + 1)`; there are no right-associative infix operators in core circom.
pub const BP_POSTFIX: u16 = 250;
/// Prefix `!`/`~`/`-` sits **between** postfix (`BP_POSTFIX`) and `**` (`BP_POWER`): the official
/// grammar tiers `Expression2` (prefix) are TIGHTER than `Expression3` (`**`), so `-a ** b` parses
/// as `(-a) ** b` (not `-(a ** b)`), and `-a.b` as `-(a.b)` (postfix binds tighter still).
pub const BP_PREFIX: u16 = 200;
pub const BP_POWER: u16 = 181;
pub const BP_MUL: u16 = 171;
pub const BP_ADD: u16 = 161;
pub const BP_SHIFT: u16 = 151;
pub const BP_BIT_AND: u16 = 141;
pub const BP_BIT_XOR: u16 = 131;
pub const BP_BIT_OR: u16 = 121;
pub const BP_CMP: u16 = 111;
pub const BP_BOOL_AND: u16 = 101;
pub const BP_BOOL_OR: u16 = 91;

/// Every circom keyword, in source order. The single source of truth for keyword text shared by
/// the lexer (`*Kw` variants) and completion. Add a new keyword here and as a `*Kw` variant.
pub const KEYWORDS: &[&str] = &[
    "pragma",
    "circom",
    "include",
    "template",
    "function",
    "component",
    "main",
    "public",
    "signal",
    "var",
    "log",
    "custom",
    "custom_templates",
    "extern_c",
    "parallel",
    "bus",
    "input",
    "output",
    "if",
    "else",
    "for",
    "while",
    "return",
    "assert",
];

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
    // Infix binding powers `(lbp, rbp)`. Returns `None` for non-infix tokens.
    //
    // The ladder follows the official circom grammar (lang.lalrpop Expression4..Expression12).
    // Notably bitwise operators bind TIGHTER than comparisons (e.g. `a & b == c` parses as
    // `(a & b) == c`), and `**` is left-associative. Every operator is left-associative, hence
    // `rbp = lbp + 1`.
    //
    // NOTE: comma is intentionally NOT an infix operator here — in circom it is only a separator
    // in argument/tuple lists, never an expression operator.
    pub fn infix(self) -> Option<(u16, u16)> {
        let lbp = match self {
            Self::Power => BP_POWER,
            Self::Mul | Self::Div | Self::IntDiv | Self::Mod => BP_MUL,
            Self::Add | Self::Sub => BP_ADD,
            Self::ShiftL | Self::ShiftR => BP_SHIFT,
            Self::BitAnd => BP_BIT_AND,
            Self::BitXor => BP_BIT_XOR,
            Self::BitOr => BP_BIT_OR,
            Self::Equal
            | Self::NotEqual
            | Self::LessThan
            | Self::GreaterThan
            | Self::LessThanAndEqual
            | Self::GreaterThanAndEqual => BP_CMP,
            Self::BoolAnd => BP_BOOL_AND,
            Self::BoolOr => BP_BOOL_OR,
            _ => return None,
        };
        Some((lbp, lbp + 1))
    }

    // priority: post > pre > in
    // associativity: right to left [ --!a --> --(!a) ]
    // prefix_operator <literal>
    // eg: -10, !a. Note: circom only allows prefix `! ~ -` (grammar ParseExpressionPrefixOpcode);
    // unary `+` and prefix `++`/`--` are NOT valid circom.
    pub fn prefix(self) -> Option<u16> {
        match self {
            Self::Not | Self::BitNot | Self::Sub => Some(BP_PREFIX),
            _ => None,
        }
    }

    // these tokens have the highest priority
    // <literal> postfix_operator
    // eg: a[10], b++, c.att1. In circom `++`/`--` are statement substitutions, not expression
    // postfix operators, so expression postfix is only call `()`, index `[]`, member `.`.
    pub fn postfix(self) -> Option<u16> {
        match self {
            Self::LParen | Self::LBracket | Self::Dot => Some(BP_POSTFIX),
            _ => None,
        }
    }

    pub fn is_declaration_kw(self) -> bool {
        matches!(
            self,
            Self::VarKw | Self::ComponentKw | Self::SignalKw
            // `input signal` / `output signal` also start a signal declaration
            | Self::InputKw | Self::OutputKw
        )
    }

    pub fn is_assign_token(self) -> bool {
        matches!(
            self,
            Self::Assign
            // signal assigment operators
            | Self::EqualSignal
            | Self::LAssignSignal
            | Self::LAssignContraintSignal
            | Self::RAssignSignal
            | Self::RAssignConstraintSignal
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
            | Self::PowerAssign // unit inc/dec
                                // | Self::UnitInc
                                // | Self::UnitDec
        )
    }

    pub fn is_inline_assign_signal(self) -> bool {
        matches!(
            self,
            Self::Assign | Self::RAssignSignal | Self::RAssignConstraintSignal
        )
    }

    pub fn is_var_assign(self) -> bool {
        matches!(self, Self::Assign)
    }

    pub fn is_trivial(self) -> bool {
        matches!(
            self,
            Self::WhiteSpace | Self::EndLine | Self::CommentLine | Self::BlockComment | Self::Error
        )
    }
}
