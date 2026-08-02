//! **Grammar-feature conformance** — every construct the official circom grammar
//! (`iden3/circom` → `parser/src/lang.lalrpop`) accepts, exercised with realistic, multi-line
//! circom programs. Each test asserts what the grammar *requires* (parse with no error, or a
//! specific token kind). These previously-`#[ignore]`d gaps now all pass under plain `cargo test`.

use parser::lexer::tokenize;
use parser::token_kind::TokenKind;
use syntax::tree::syntax_tree;

// --- shared harness ---------------------------------------------------------------------------

/// Parse `src` as a full program; true iff the tree has no `Error`/`ParserError` node.
fn parses_clean(src: &str) -> bool {
    let root = syntax_tree(src);
    !root
        .descendants()
        .any(|n| n.kind() == TokenKind::Error || n.kind() == TokenKind::ParserError)
}

/// Non-trivial token kinds (trivia dropped).
fn kinds(src: &str) -> Vec<TokenKind> {
    tokenize(src)
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| !k.is_trivial())
        .collect::<Vec<_>>()
}

// =====================================================================================
// A. Lexer terminals (grammar section "Terminals")
// =====================================================================================

#[test]
fn gap_hexnumber_zero_digits() {
    // Grammar: `0x` is a valid HEXNUMBER (regex `0x[0-9A-Fa-f]*`, zero digits allowed).
    let src = "template T() { signal input a; signal output o; o <== a + 0x; }";
    assert!(
        parses_clean(src),
        "`0x` should be a valid (zero-digit) hex literal"
    );
}

#[test]
fn gap_string_is_single_line() {
    // Grammar: a newline inside a string terminates it (strings are single-line). With the
    // `"[^"\n]*"` regex a multi-line literal cannot form a single CircomString token.
    let bad = "\"line one\nline two\"";
    assert!(
        !tokenize(bad)
            .iter()
            .any(|t| t.kind == TokenKind::CircomString),
        "a newline inside a string literal should prevent a CircomString token"
    );
}

#[test]
fn gap_parallel_is_a_keyword() {
    // `parallel` is reserved (Expression14 + template modifier), not an Identifier.
    assert_ne!(kinds("parallel").as_slice(), &[TokenKind::Identifier]);
}

#[test]
fn gap_custom_is_a_keyword() {
    assert_ne!(kinds("custom").as_slice(), &[TokenKind::Identifier]);
}

#[test]
fn gap_extern_c_is_a_keyword() {
    assert_ne!(kinds("extern_c").as_slice(), &[TokenKind::Identifier]);
}

#[test]
fn gap_bus_is_a_keyword() {
    assert_ne!(kinds("bus").as_slice(), &[TokenKind::Identifier]);
}

#[test]
fn gap_custom_templates_is_a_keyword() {
    assert_ne!(
        kinds("custom_templates").as_slice(),
        &[TokenKind::Identifier]
    );
}

// =====================================================================================
// B. Pragmas (grammar ParsePragma)
// =====================================================================================

#[test]
fn gap_pragma_custom_templates() {
    let src = "pragma circom 2.0.0;\npragma custom_templates;\ntemplate T() { signal input a; }";
    assert!(parses_clean(src));
}

// =====================================================================================
// C. Definitions (grammar ParseDefinition)
// =====================================================================================

#[test]
fn gap_template_parallel_modifier() {
    let src = "pragma circom 2.0.0;
template parallel ParallelHash(n) {
    signal input in[n];
    signal output out;
    component hashes[n];
    var acc = 0;
    for (var i = 0; i < n; i++) {
        hashes[i] = Hash()(in[i]);
        acc += hashes[i].out;
    }
    out <== acc;
}";
    assert!(parses_clean(src));
}

#[test]
fn gap_template_custom_modifier() {
    let src = "pragma circom 2.0.0;\npragma custom_templates;\ntemplate custom Poseidon(n) {\n    signal input x;\n    signal output y;\n    y <== x;\n}";
    assert!(parses_clean(src));
}

#[test]
fn gap_template_extern_c_modifier() {
    let src = "pragma circom 2.0.0;\ntemplate extern_c FieldMul() {\n    signal input a;\n    signal input b;\n    signal output c;\n    c <== a * b;\n}";
    assert!(parses_clean(src));
}

#[test]
fn gap_template_combined_modifiers() {
    // Grammar fixed modifier order: `custom` → `extern_c` → `parallel`.
    let src =
        "pragma circom 2.0.0;\ntemplate custom extern_c parallel G() { signal output o; o <== 0; }";
    assert!(parses_clean(src));
}

#[test]
fn gap_bus_definition_no_params() {
    let src = "pragma circom 2.0.0;
bus Point {
    signal input x;
    signal input y;
}
template T() {
    signal output o;
    o <== 0;
}";
    assert!(parses_clean(src));
}

#[test]
fn gap_bus_definition_with_params() {
    let src = "pragma circom 2.0.0;
bus Vector(n) {
    signal input items[n];
}
template T() {
    signal output o;
    o <== 0;
}";
    assert!(parses_clean(src));
}

// =====================================================================================
// D. Declarations (grammar ParseDeclaration / BusHeader) — bus-typed signals
// =====================================================================================

#[test]
fn gap_bus_typed_signal() {
    // Grammar BusHeader (wire-first): `<input|output> <BusType> <fieldName>`.
    let src = "pragma circom 2.0.0;
bus B { signal input x; }
template T() {
    input B b;
    signal output o;
    o <== b.x;
}";
    assert!(parses_clean(src));
}

#[test]
fn gap_bus_typed_signal_with_args() {
    // BusHeader with instantiation args: `input <Bus>(args) <fieldName>`.
    let src = "pragma circom 2.0.0;
bus V(n) { signal input items[n]; }
template T(k) {
    input V(k) v;
    signal output o;
    o <== 0;
}";
    assert!(parses_clean(src));
}

// =====================================================================================
// E. Expressions (grammar Expression0..Expression14)
// =====================================================================================

#[test]
fn gap_parallel_expression() {
    // `parallel <expr>` wraps an expression for parallel evaluation (grammar Expression14).
    let src = "template T() {
    signal input a;
    signal input b;
    signal input c;
    var acc = a + b + c;
    signal output out;
    out <== parallel acc;
}";
    assert!(parses_clean(src));
}

#[test]
fn gap_inline_array_expression() {
    // grammar Expression1 inline array: `[a, b, c]`.
    let src = "template T() {
    signal input a;
    signal input b;
    signal input c;
    var arr = [a, b, c];
    signal output out;
    out <== arr[0] + arr[1] + arr[2];
}";
    assert!(parses_clean(src));
}

#[test]
fn gap_tuple_expression() {
    // grammar Expression1 tuple: `(a, b)` (≥2 elements).
    let src = "template T() {
    signal input a;
    signal input b;
    var (x, y) = (a, b);
    signal output out;
    out <== x + y;
}";
    assert!(parses_clean(src));
}

#[test]
fn gap_underscore_placeholder() {
    // grammar Expression0: `_` placeholder variable (anonymous assignment target).
    let src = "template T() {
    signal input a;
    signal output out;
    _ <== a;
    out <== a;
}";
    assert!(parses_clean(src));
}

// =====================================================================================
// F. Statements (grammar ParseStatement2)
// =====================================================================================

// (Anonymous component statements `T(args)(signals);` already parse — see
// `grammar_structure.rs::anonymous_component_*`.)

// =====================================================================================
// G. Whole-program realism — a complicated circuit mixing many grammar features at once.
//    This is the "must parse" end-to-end target.
// =====================================================================================

#[test]
fn gap_complicated_realistic_program() {
    let src = r#"pragma circom 2.0.0;
pragma custom_templates;

bus Inputs(n) {
    signal input a[n];
    signal input b[n];
}

template custom Poseidon(n) {
    signal input state;
    signal output out;
    out <== state;
}

template parallel MerkleTree(levels) {
    signal input leaf;
    signal input path[levels];
    signal output root;

    var acc = leaf;
    for (var i = 0; i < levels; i++) {
        acc <== Poseidon(1)(acc + path[i]);
    }
    root <== parallel acc;
}

template Aggregator(n) {
    input Inputs(n) inp;
    signal output hash;
    var pair = (Inputs.a[0], Inputs.b[0]);
    component leaf = Multiplier2();
    Multiplier2()(pair);
    var proof = [Inputs.a[0], Inputs.b[0]];
    hash <== MerkleTree(n)(leaf.out, proof);
}

component main {public [hash]} = Aggregator(4);
"#;
    assert!(parses_clean(src));
}
