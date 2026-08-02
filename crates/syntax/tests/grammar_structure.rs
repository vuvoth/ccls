//! Grammar-structure conformance with the official circom grammar
//! (`iden3/circom` → `parser/src/lang.lalrpop`).
//!
//! These tests assert the **shape** our parser produces for each grammar production — derived
//! from the EBNF, not from round-tripping our own output. Expression precedence/associativity is
//! checked structurally (which operator is outermost = loosest-binding), which is where parser
//! bugs hide. Known grammar features we do not yet support are pinned as `#[ignore]`d tests so the
//! gap is documented and surfaces the moment support lands.

use parser::token_kind::TokenKind;
use syntax::syntax::syntax_tree;
use syntax::syntax_node::SyntaxNode;

// --- harness ------------------------------------------------------------------------------------

/// Parse `src` as a whole program and return the root node.
fn program(src: &str) -> SyntaxNode {
    syntax_tree(src)
}

/// Parse `src` as the body of a template (declaration/statement constructs live in a block).
fn in_block(src: &str) -> SyntaxNode {
    syntax_tree(&format!("pragma circom 2.0.0;\ntemplate T() {{\n{src}\n}}"))
}

/// Parse `expr` as the RHS of `var x = <expr>;` and return the program root.
fn with_expr(expr: &str) -> SyntaxNode {
    in_block(&format!("var x = {expr};"))
}

/// Does the tree contain a node of `kind`?
fn has_kind(root: &SyntaxNode, kind: TokenKind) -> bool {
    root.descendants().any(|n| n.kind() == kind)
}

/// Does the tree contain any error node (`Error` or `ParserError`)?
fn has_error(root: &SyntaxNode) -> bool {
    root.descendants()
        .any(|n| n.kind() == TokenKind::Error || n.kind() == TokenKind::ParserError)
}

/// The first `Expression` node in the tree (the RHS of a `var x = …;`).
fn first_expression(root: &SyntaxNode) -> SyntaxNode {
    root.descendants()
        .find(|n| n.kind() == TokenKind::Expression)
        .expect("an Expression node should be present")
}

/// The outermost operator node of an expression: the single child of its `Expression` wrapper.
/// For `a + b` this is the `Add` node; for a bare atom `a` it is the `ExpressionAtom`.
fn outer_node(root: &SyntaxNode) -> SyntaxNode {
    let expr = first_expression(root);
    expr.children()
        .next()
        .expect("Expression must have a child node")
}

/// The kind of the outermost operator of `expr`, or `None` if `expr` is a bare atom.
fn outermost_op(expr: &str) -> Option<TokenKind> {
    let node = outer_node(&with_expr(expr));
    (node.kind().infix().is_some() || node.kind().prefix().is_some()).then_some(node.kind())
}

/// True when a same-precedence binary chain groups left-associatively: `(a OP b) OP c`.
/// Checks that the outermost operator's left operand is itself an operator node.
fn is_left_assoc(expr: &str) -> bool {
    let outer = outer_node(&with_expr(expr));
    if outer.kind().infix().is_none() {
        return false;
    }
    outer
        .children()
        .next()
        .map(|left| left.kind().infix().is_some())
        .unwrap_or(false)
}

// =====================================================================================
// Expression precedence tiers (grammar Expression0..Expression13), tight→loose:
//   postfix(1) > prefix(2) > **(3) > * / \ %(4) > + -(5) > << >>(6) > &(7) >
//   ^(8) > |(9) > cmp(10) > &&(11) > ||(12) > ?: (13)
// The outermost operator of an expression is the LOOSEST-binding operator present.
// =====================================================================================

#[test]
fn mul_binds_tighter_than_add() {
    // a + b * c  ==  a + (b * c)  → outermost is `+`
    assert_eq!(outermost_op("a + b * c"), Some(TokenKind::Add));
    assert_eq!(outermost_op("a * b + c"), Some(TokenKind::Add));
}

#[test]
fn bitwise_binds_tighter_than_comparison() {
    // grammar Expression7(&) tighter than Expression10(==): a & b == c == (a & b) == c
    assert_eq!(outermost_op("a & b == c"), Some(TokenKind::Equal));
    assert_eq!(outermost_op("a == b & c"), Some(TokenKind::Equal));
}

#[test]
fn bitxor_binds_tighter_than_bitor() {
    // Expression8(^) tighter than Expression9(|)
    assert_eq!(outermost_op("a | b ^ c"), Some(TokenKind::BitOr));
}

#[test]
fn bitand_binds_tighter_than_bitxor() {
    // Expression7(&) tighter than Expression8(^)
    assert_eq!(outermost_op("a ^ b & c"), Some(TokenKind::BitXor));
}

#[test]
fn add_binds_tighter_than_shift() {
    // Expression5(+/-) tighter than Expression6(<<>>): a + b << c == (a + b) << c
    assert_eq!(outermost_op("a + b << c"), Some(TokenKind::ShiftL));
}

#[test]
fn bool_or_is_loosest_binary() {
    // Expression12(||) looser than Expression11(&&): a || b && c == a || (b && c)
    assert_eq!(outermost_op("a || b && c"), Some(TokenKind::BoolOr));
    assert_eq!(outermost_op("a && b || c"), Some(TokenKind::BoolOr));
}

#[test]
fn prefix_is_tighter_than_power() {
    // grammar Expression2(prefix ! ~ -) is TIGHTER than Expression3(**): -a ** b == (-a) ** b.
    // (Previously our parser had this backwards — `-(a ** b)` — a real conformance bug.)
    assert_eq!(
        outermost_op("-a ** b"),
        Some(TokenKind::Power),
        "-a ** b must parse as (-a) ** b per the grammar (prefix tighter than **)"
    );
    // Prefix is also tighter than `*`/`+`: -a * b == (-a) * b, !a && b == (!a) && b.
    assert_eq!(outermost_op("-a * b"), Some(TokenKind::Mul));
    assert_eq!(outermost_op("!a && b"), Some(TokenKind::BoolAnd));
}

#[test]
fn postfix_is_tighter_than_prefix() {
    // grammar Expression1(postfix . [] ()) tighter than Expression2(prefix): -a.b == -(a.b)
    assert_eq!(outermost_op("-a.b"), Some(TokenKind::Sub));
}

#[test]
fn ternary_is_loosest() {
    // Expression13(?:) is the loosest operator (below parallel, which we don't support).
    assert_eq!(
        outer_node(&with_expr("a ? b : c")).kind(),
        TokenKind::TernaryConditional
    );
}

// --- associativity: every circom infix operator is LEFT-associative (InfixOpTier is
//     left-recursive), including `**`. ---------------------------------------------------

#[test]
fn subtraction_is_left_associative() {
    assert!(is_left_assoc("a - b - c"), "a - b - c == (a - b) - c");
}

#[test]
fn division_is_left_associative() {
    assert!(is_left_assoc("a / b / c"));
}

#[test]
fn power_is_left_associative() {
    // grammar Expression3 = InfixOpTier<Exp, Expression2> is left-recursive → (a ** b) ** c
    assert!(is_left_assoc("a ** b ** c"), "** must be left-associative");
}

#[test]
fn mixed_add_sub_left_associative_and_outermost_is_last_op() {
    // a - b + c == (a - b) + c → outermost is `+`, and it groups left.
    assert_eq!(outermost_op("a - b + c"), Some(TokenKind::Add));
    assert!(is_left_assoc("a - b + c"));
}

#[test]
fn nested_ternary_is_rejected() {
    // grammar Expression13 branches are Expression12 — a nested ternary `c ? d : e` inside a
    // branch is a syntax error in circom. The leftover `? d : e` must surface as an error.
    let root = with_expr("a ? b : c ? d : e");
    assert!(
        has_error(&root),
        "nested ternary `a ? b : c ? d : e` must be an error"
    );
}

#[test]
fn anonymous_component_expression_parses() {
    // grammar Expression1: `T(params)(signal_args)` — an anonymous component instantiation. Our
    // parser models it as two nested postfix `Call`s, which parses cleanly (a dedicated
    // `AnonymousComponent` node kind is a future refinement, not a parse gap).
    let src =
        "template Multiplier2() { signal input a; signal input b; signal output c; c <== a * b; }
template T() {
    signal input x;
    signal input y;
    signal output out;
    out <== Multiplier2()(x, y);
}";
    assert!(!has_error(&program(src)));
}

#[test]
fn anonymous_component_statement_parses() {
    // grammar ParseStatement2: an anonymous component used as a statement `T(args)(signals);`.
    let src = "template Foo() { signal input a; signal output b; b <== a; }
template T() {
    signal input x;
    Foo()(x);
    signal output out;
    out <== x;
}";
    assert!(!has_error(&program(src)));
}

// =====================================================================================
// Declarations (grammar ParseDeclaration / SignalHeader)
// =====================================================================================

#[test]
fn signal_header_both_keyword_orders() {
    // grammar SignalHeader: `signal (input|output)?` OR `(input|output) signal`, + optional tags.
    assert!(has_kind(
        &in_block("signal input a;"),
        TokenKind::InputSignalDecl
    ));
    assert!(has_kind(
        &in_block("signal output b;"),
        TokenKind::OutputSignalDecl
    ));
    assert!(has_kind(&in_block("signal c;"), TokenKind::SignalDecl));
    assert!(has_kind(
        &in_block("input signal d;"),
        TokenKind::InputSignalDecl
    ));
    assert!(has_kind(
        &in_block("output signal e;"),
        TokenKind::OutputSignalDecl
    ));
}

#[test]
fn signal_tag_list() {
    // grammar ParseTagsList: tags precede the name — `signal { t1, t2 } x;`
    let root = in_block("signal {t1, t2} x;");
    assert!(has_kind(&root, TokenKind::SignalDecl));
    assert!(!has_error(&root));
}

#[test]
fn signal_inline_init() {
    // since circom 2.0.4, intermediate/output signals may initialize inline: `signal out <== e;`
    let root = in_block("signal output out <== a + b;");
    assert!(has_kind(&root, TokenKind::OutputSignalDecl));
    assert!(!has_error(&root));
}

#[test]
fn var_and_component_declarations() {
    assert!(has_kind(&in_block("var x = 1;"), TokenKind::VarDecl));
    assert!(has_kind(
        &in_block("var (a, b) = (1, 2);"),
        TokenKind::VarDecl
    ));
    assert!(has_kind(
        &in_block("component c = T();"),
        TokenKind::ComponentDecl
    ));
    assert!(has_kind(
        &in_block("component arr[N];"),
        TokenKind::ComponentDecl
    ));
}

// =====================================================================================
// Statements (grammar ParseSubstitution / ParseStatement2)
// =====================================================================================

#[test]
fn substitution_operators() {
    // grammar ParseSubstitution: `= <-- <==` plus compound `+= -= *= /= \= %= **= <<= >>= &= |= ^=`.
    for stmt in [
        "a = b;", "a <-- b;", "a <== b;", "a += b;", "a -= b;", "a *= b;", "a /= b;", "a %= b;",
        "a **= b;", "a <<= b;", "a >>= b;", "a &= b;", "a |= b;", "a ^= b;",
    ] {
        let root = in_block(stmt);
        assert!(
            has_kind(&root, TokenKind::AssignStatement),
            "missing AssignStatement: {stmt}"
        );
        assert!(!has_error(&root), "unexpected error: {stmt}");
    }
}

#[test]
fn postfix_increment_decrement() {
    // grammar: `var ++` / `var --` are statement-level (postfix); prefix `++`/`--` are illegal.
    assert!(has_kind(&in_block("a++;"), TokenKind::AssignStatement));
    assert!(has_kind(&in_block("a--;"), TokenKind::AssignStatement));
    assert!(
        has_error(&in_block("++a;")),
        "prefix `++a` is illegal in circom"
    );
}

#[test]
fn control_flow_statements() {
    assert!(has_kind(
        &in_block("for (var i = 0; i < n; i++) {}"),
        TokenKind::ForLoop
    ));
    assert!(has_kind(
        &in_block("while (a < b) {}"),
        TokenKind::WhileLoop
    ));
    assert!(has_kind(
        &in_block("if (a) {} else {}"),
        TokenKind::IfStatement
    ));
    assert!(has_kind(
        &in_block("assert(a == b);"),
        TokenKind::AssertStatement
    ));
    assert!(has_kind(&in_block("return x;"), TokenKind::ReturnStatement));
    assert!(has_kind(
        &in_block("log(\"s\", a);"),
        TokenKind::LogStatement
    ));
}

// =====================================================================================
// Top-level (grammar ParseAst / ParseDefinition / ParseMainComponent)
// =====================================================================================

#[test]
fn top_level_constructs() {
    let src = "pragma circom 2.0.0;
include \"lib.circom\";
template T() { signal input a; }
function f(x) { return x; }
component main = T();
";
    let root = program(src);
    assert!(has_kind(&root, TokenKind::Pragma));
    assert!(has_kind(&root, TokenKind::Include));
    assert!(has_kind(&root, TokenKind::TemplateDef));
    assert!(has_kind(&root, TokenKind::FunctionDef));
    assert!(has_kind(&root, TokenKind::MainComponent));
    assert!(!has_error(&root));
}

#[test]
fn main_component_public_list() {
    // grammar ParsePublicList: `component main {public [a, b]} = X();`
    let root = program("component main {public [a, b]} = X();");
    assert!(has_kind(&root, TokenKind::MainComponent));
    assert!(!has_error(&root));
}

#[test]
fn top_level_allows_any_order() {
    // grammar ParseAst permits pragma/include/definition/main interspersed (conventional prefix
    // is not required).
    let root = program("template T() {}\ninclude \"a.circom\";\npragma circom 2.0.0;\n");
    assert!(has_kind(&root, TokenKind::Include));
    assert!(!has_error(&root));
}

// The exhaustive inventory of grammar features we do NOT yet support lives in
// `tests/grammar_gaps.rs` (each a complicated, realistic circom program, `#[ignore]`d so CI stays
// green; run `cargo test --test grammar_gaps -- --ignored` to filter the full missing list).
