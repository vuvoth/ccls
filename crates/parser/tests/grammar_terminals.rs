//! Lexer conformance with the official circom grammar terminals
//! (`iden3/circom` → `parser/src/lang.lalrpop`, section "Terminals").
//!
//! Each test asserts the kind a terminal produces, derived directly from the grammar's regex —
//! NOT from our parser's behavior. Gaps (where our lexer deviates from the grammar) are pinned
//! explicitly so any change is a visible decision.

use parser::lexer::tokenize;
use parser::token_kind::TokenKind;

/// Non-trivial token kinds (trivia dropped) — the sequence the parser actually consumes.
fn kinds(src: &str) -> Vec<TokenKind> {
    tokenize(src)
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| !k.is_trivial())
        .collect::<Vec<_>>()
}

// --- IDENTIFIER: grammar `r"[$_]*[a-zA-Z][a-zA-Z$_0-9]*"` ---------------------------------------

#[test]
fn identifier_terminal_conforms() {
    // Must start with `$_*` then a letter; `$`/`_` may also appear mid-identifier.
    for ok in [
        "a", "Abc", "x1", "_foo", "$bar", "a_b", "a$b", "A1_$", "_$_x",
    ] {
        assert_eq!(
            kinds(ok),
            vec![TokenKind::Identifier],
            "failed ident: {ok:?}"
        );
    }
}

#[test]
fn identifier_requires_a_letter() {
    // Grammar IDENTIFIER needs at least one letter: `_` / `$` alone are NOT identifiers.
    for bad in ["_", "$", "$_", "1abc"] {
        assert_ne!(
            kinds(bad).as_slice(),
            &[TokenKind::Identifier],
            "`{bad}` must not be a lone identifier per grammar"
        );
    }
    // A leading digit splits the token (number cannot include a letter): `123abc` -> Number, Ident.
    assert_eq!(
        kinds("123abc"),
        vec![TokenKind::Number, TokenKind::Identifier]
    );
}

// --- DECNUMBER: `r"[0-9]+"` --------------------------------------------------------------------

#[test]
fn decnumber_terminal_conforms() {
    for ok in ["0", "7", "42", "1000000"] {
        assert_eq!(kinds(ok), vec![TokenKind::Number], "failed number: {ok:?}");
    }
}

// --- HEXNUMBER: grammar `r"0x[0-9A-Fa-f]*"` ----------------------------------------------------
// The grammar allows ZERO hex digits (`*`), so `0x` alone is a valid HEXNUMBER.

#[test]
fn hexnumber_terminal_conforms() {
    for ok in ["0x0", "0x1F", "0xDEADBEEF", "0xabcdef", "0xCaFe"] {
        assert_eq!(kinds(ok), vec![TokenKind::HexNumber], "failed hex: {ok:?}");
    }
}

#[test]
fn hexnumber_empty_digits_conform() {
    // Grammar: `0x` alone is a HEXNUMBER (`0x[0-9A-Fa-f]*`, zero digits allowed).
    assert_eq!(
        kinds("0x"),
        vec![TokenKind::HexNumber],
        "`0x` must be a valid (zero-digit) HexNumber per the grammar"
    );
}

// --- STRING: grammar `r#""[^"\n]*""#` (no newlines) --------------------------------------------

#[test]
fn string_terminal_conforms() {
    assert_eq!(kinds(r#""hello""#), vec![TokenKind::CircomString]);
    assert_eq!(kinds(r#""""#), vec![TokenKind::CircomString]); // empty string
    assert_eq!(kinds(r#""a b c""#), vec![TokenKind::CircomString]);
}

#[test]
fn string_newline_terminates() {
    // Grammar: a newline inside a string terminates it (strings are single-line). The newline
    // breaks the `"[^"\n]*"` match, so no single CircomString spans the line break.
    assert!(
        !tokenize("\"line\nbreak\"")
            .iter()
            .any(|t| t.kind == TokenKind::CircomString),
        "a newline inside a string literal must prevent a CircomString token"
    );
}

// --- Version: `SMALL "." SMALL "." SMALL` where SMALL = `[0-9]+` -------------------------------

#[test]
fn version_terminal_conforms() {
    for ok in ["2.0.0", "2.1.12", "22.0.0"] {
        assert_eq!(
            kinds(ok),
            vec![TokenKind::Version],
            "failed version: {ok:?}"
        );
    }
}

#[test]
fn version_requires_three_parts() {
    // `2.0` is not a valid Version (grammar needs N.N.N); circom has no floats.
    assert_ne!(kinds("2.0").as_slice(), &[TokenKind::Version]);
}

// --- Operators: maximal-munch from the grammar's infix/prefix/assign/substitution op sets -------

#[test]
fn multi_char_operators_are_one_token() {
    // Each of these is a single grammar terminal / operator; maximal munch must win over prefixes.
    let cases: &[(TokenKind, &str)] = &[
        (TokenKind::BoolAnd, "&&"),
        (TokenKind::BoolOr, "||"),
        (TokenKind::Equal, "=="),
        (TokenKind::NotEqual, "!="),
        (TokenKind::LessThanAndEqual, "<="),
        (TokenKind::GreaterThanAndEqual, ">="),
        (TokenKind::Power, "**"),
        (TokenKind::ShiftL, "<<"),
        (TokenKind::ShiftR, ">>"),
        (TokenKind::EqualSignal, "==="),
        (TokenKind::LAssignSignal, "-->"),
        (TokenKind::LAssignContraintSignal, "==>"),
        (TokenKind::RAssignSignal, "<--"),
        (TokenKind::RAssignConstraintSignal, "<=="),
        (TokenKind::IntDivAssign, r"\="),
        (TokenKind::PowerAssign, "**="),
        (TokenKind::AddAssign, "+="),
        (TokenKind::SubAssign, "-="),
        (TokenKind::MulAssign, "*="),
        (TokenKind::DivAssign, "/="),
        (TokenKind::ModAssign, "%="),
        (TokenKind::ShiftLAssign, "<<="),
        (TokenKind::ShiftRAssign, ">>="),
        (TokenKind::BitAndAssign, "&="),
        (TokenKind::BitOrAssign, "|="),
        (TokenKind::BitXorAssign, "^="),
        (TokenKind::UnitInc, "++"),
        (TokenKind::UnitDec, "--"),
    ];
    for (kind, src) in cases {
        assert_eq!(kinds(src), vec![*kind], "failed operator: {src:?}");
    }
}

// --- Keywords: the grammar's reserved words ----------------------------------------------------

#[test]
fn keyword_terminals_conform() {
    let cases: &[(TokenKind, &str)] = &[
        (TokenKind::PragmaKw, "pragma"),
        (TokenKind::Circom, "circom"),
        (TokenKind::IncludeKw, "include"),
        (TokenKind::TemplateKw, "template"),
        (TokenKind::FunctionKw, "function"),
        (TokenKind::ComponentKw, "component"),
        (TokenKind::MainKw, "main"),
        (TokenKind::PublicKw, "public"),
        (TokenKind::SignalKw, "signal"),
        (TokenKind::VarKw, "var"),
        (TokenKind::InputKw, "input"),
        (TokenKind::OutputKw, "output"),
        (TokenKind::LogKw, "log"),
        (TokenKind::IfKw, "if"),
        (TokenKind::ElseKw, "else"),
        (TokenKind::ForKw, "for"),
        (TokenKind::WhileKw, "while"),
        (TokenKind::ReturnKw, "return"),
        (TokenKind::AssertKw, "assert"),
        (TokenKind::CustomKw, "custom"),
        (TokenKind::CustomTemplatesKw, "custom_templates"),
        (TokenKind::ExternCKw, "extern_c"),
        (TokenKind::ParallelKw, "parallel"),
        (TokenKind::BusKw, "bus"),
    ];
    for (kind, src) in cases {
        assert_eq!(kinds(src), vec![*kind], "failed keyword: {src:?}");
    }
}

// --- Punctuation / brackets --------------------------------------------------------------------

#[test]
fn punctuation_terminals_conform() {
    let cases: &[(TokenKind, &str)] = &[
        (TokenKind::LParen, "("),
        (TokenKind::RParen, ")"),
        (TokenKind::LCurly, "{"),
        (TokenKind::RCurly, "}"),
        (TokenKind::LBracket, "["),
        (TokenKind::RBracket, "]"),
        (TokenKind::Semicolon, ";"),
        (TokenKind::Comma, ","),
        (TokenKind::Dot, "."),
        (TokenKind::Colon, ":"),
        (TokenKind::MarkQuestion, "?"),
        (TokenKind::Assign, "="),
        (TokenKind::Add, "+"),
        (TokenKind::Sub, "-"),
        (TokenKind::Mul, "*"),
        (TokenKind::Div, "/"),
        (TokenKind::IntDiv, r"\"),
        (TokenKind::Mod, "%"),
        (TokenKind::BitAnd, "&"),
        (TokenKind::BitOr, "|"),
        (TokenKind::BitXor, "^"),
        (TokenKind::BitNot, "~"),
        (TokenKind::Not, "!"),
        (TokenKind::LessThan, "<"),
        (TokenKind::GreaterThan, ">"),
    ];
    for (kind, src) in cases {
        assert_eq!(kinds(src), vec![*kind], "failed punct: {src:?}");
    }
}
