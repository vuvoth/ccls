//! **Missing-grammar-feature inventory** — every construct the official circom grammar
//! (`iden3/circom` → `parser/src/lang.lalrpop`) accepts but our parser does NOT yet support.
//!
//! Each test uses a realistic, multi-line circom program (not a toy snippet) exercising the
//! feature in context, and asserts what the grammar *requires* (parse with no error, or a specific
//! node kind). They are `#[ignore]`d so `cargo test` stays green; **the filter** is:
//!
//! ```text
//! cargo test --test grammar_gaps -- --ignored
//! ```
//!
//! Every test that still FAILS under `--ignored` is a real gap to implement; any that PASS are
//! features we already handle (remove the `#[ignore]` and promote them to `grammar_structure.rs`).
//! Implementation is tracked separately; this file is the authoritative "what are we missing" list.

use parser::lexer::tokenize;
use parser::token_kind::TokenKind;
use syntax::syntax::syntax_tree;

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
#[ignore = "grammar HEXNUMBER = `0x[0-9A-Fa-f]*` allows zero hex digits (`0x` alone)"]
fn gap_hexnumber_zero_digits() {
    // Grammar: `0x` is a valid HEXNUMBER. Complicated: a bus field initialized to `0x`.
    let src = "template T() { signal input a; signal output o; o <== a + 0x; }";
    assert!(
        parses_clean(src),
        "`0x` should be a valid (zero-digit) hex literal"
    );
}

#[test]
#[ignore = "grammar STRING = `\"[^\"\\n]*\"` is single-line; a newline must terminate it"]
fn gap_string_is_single_line() {
    // Grammar: a newline inside a string is invalid. We over-accept multi-line strings.
    let bad = "\"line one\nline two\"";
    assert!(
        kinds(bad).contains(&TokenKind::Error),
        "a newline inside a string literal should be a lexing error"
    );
}

#[test]
#[ignore = "grammar reserves `parallel` (Expression14 + template modifier) — we lex it as Identifier"]
fn gap_parallel_is_a_keyword() {
    // If `parallel` were reserved, `template parallel T()` dispatches to the modifier arm. Instead
    // it lexes as a plain Identifier, so the modifier keyword is never recognized.
    assert_ne!(kinds("parallel").as_slice(), &[TokenKind::Identifier]);
}

#[test]
#[ignore = "grammar reserves `custom` (template modifier) — we lex it as Identifier"]
fn gap_custom_is_a_keyword() {
    assert_ne!(kinds("custom").as_slice(), &[TokenKind::Identifier]);
}

#[test]
#[ignore = "grammar reserves `extern_c` (template modifier) — we lex it as Identifier"]
fn gap_extern_c_is_a_keyword() {
    assert_ne!(kinds("extern_c").as_slice(), &[TokenKind::Identifier]);
}

#[test]
#[ignore = "grammar reserves `bus` (ParseDefinition) — we lex it as Identifier"]
fn gap_bus_is_a_keyword() {
    assert_ne!(kinds("bus").as_slice(), &[TokenKind::Identifier]);
}

#[test]
#[ignore = "grammar reserves `custom_templates` (ParsePragma) — we lex it as Identifier(s)"]
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
#[ignore = "grammar ParsePragma: `pragma custom_templates;` enables custom gates"]
fn gap_pragma_custom_templates() {
    let src = "pragma circom 2.0.0;\npragma custom_templates;\ntemplate T() { signal input a; }";
    assert!(parses_clean(src));
}

// =====================================================================================
// C. Definitions (grammar ParseDefinition)
// =====================================================================================

#[test]
#[ignore = "grammar ParseDefinition: `template parallel T() {}`"]
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
#[ignore = "grammar ParseDefinition: `template custom T() {}` (custom gate)"]
fn gap_template_custom_modifier() {
    let src = "pragma circom 2.0.0;\npragma custom_templates;\ntemplate custom Poseidon(n) {\n    signal input x;\n    signal output y;\n    y <== x;\n}";
    assert!(parses_clean(src));
}

#[test]
#[ignore = "grammar ParseDefinition: `template extern_c T() {}`"]
fn gap_template_extern_c_modifier() {
    let src = "pragma circom 2.0.0;\ntemplate extern_c FieldMul() {\n    signal input a;\n    signal input b;\n    signal output c;\n    c <== a * b;\n}";
    assert!(parses_clean(src));
}

#[test]
#[ignore = "grammar ParseDefinition: combined modifiers `template parallel custom extern_c T() {}`"]
fn gap_template_combined_modifiers() {
    let src =
        "pragma circom 2.0.0;\ntemplate parallel custom extern_c G() { signal output o; o <== 0; }";
    assert!(parses_clean(src));
}

#[test]
#[ignore = "grammar ParseDefinition: `bus Name { ... }` definition (no params)"]
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
#[ignore = "grammar ParseDefinition + BusHeader: `bus Name(args) { ... }` with params"]
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
#[ignore = "grammar BusHeader: bus-typed signal `signal bus B;` / `signal input bus B;`"]
fn gap_bus_typed_signal() {
    let src = "pragma circom 2.0.0;
bus B { signal input x; }
template T() {
    signal input bus B;
    signal output o;
    o <== B.x;
}";
    assert!(parses_clean(src));
}

#[test]
#[ignore = "grammar BusHeader: `bus B(args)`-typed signal with instantiation args"]
fn gap_bus_typed_signal_with_args() {
    let src = "pragma circom 2.0.0;
bus V(n) { signal input items[n]; }
template T(k) {
    signal input bus V(k);
    signal output o;
    o <== 0;
}";
    assert!(parses_clean(src));
}

// =====================================================================================
// E. Expressions (grammar Expression0..Expression14)
// =====================================================================================

#[test]
#[ignore = "grammar Expression14: `parallel <expr>` wraps an expression for parallel evaluation"]
fn gap_parallel_expression() {
    // NOTE: `parallel (a + b)` would FALSELY parse as a function call today (since `parallel` is
    // not reserved), so we use a bare operand to make the gap observable: once `parallel` is a
    // keyword + Expression14 is handled, this parses; today it errors on the trailing operand.
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
#[ignore = "grammar Expression1: inline array `[a, b, c]`"]
fn gap_inline_array_expression() {
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
#[ignore = "grammar Expression1: tuple expression `(a, b)` (2+ elements)"]
fn gap_tuple_expression() {
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
#[ignore = "grammar Expression0: `_` placeholder variable (anonymous assignment target)"]
fn gap_underscore_placeholder() {
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
//    This is the "must parse" end-to-end target once the gaps above are closed.
// =====================================================================================

#[test]
#[ignore = "end-to-end: a realistic circuit using buses, parallel, custom gates, anon components, arrays, tuples"]
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
    signal input bus Inputs(n);
    signal output hash;
    var pair = (Inputs.a[0], Inputs.b[0]);
    component leaf = Multiplier2()(pair.0, pair.1);
    var proof = [Inputs.a[0], Inputs.b[0]];
    hash <== MerkleTree(n)(leaf.out, proof);
}

component main {public [hash]} = Aggregator(4);
"#;
    assert!(parses_clean(src));
}
