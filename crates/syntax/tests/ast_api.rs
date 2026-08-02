//! Typed-AST conformance for the new grammar features. Where `grammar_structure.rs` checks the raw
//! node *shape* and `grammar_gaps.rs` checks *parses_clean*, these tests drive the typed `AstNode`
//! accessors and assert the actual parsed *values* (names, parameters, element lists) — the real
//! proof that the parser produces semantically correct trees, not merely error-free ones.

use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::{
    AstCircomProgram, AstInlineArray, AstParallelExpr, AstTupleExpr, Named,
};
use syntax::node::{CircomLanguage, SyntaxNode};
use syntax::tree::syntax_tree;

/// Parse `src` as a full program and return the typed root.
fn program(src: &str) -> AstCircomProgram {
    let root: SyntaxNode = syntax_tree(src);
    AstCircomProgram::cast(root).expect("root must be a CircomProgram")
}

/// The first descendant node casting to `N`.
fn first<N: AstNode<Language = CircomLanguage>>(root: &SyntaxNode) -> N {
    root.descendants()
        .find_map(|n| N::cast(n))
        .unwrap_or_else(|| panic!("a {} node should be present", std::any::type_name::<N>()))
}

fn has_error(root: &SyntaxNode) -> bool {
    root.descendants()
        .any(|n| n.kind() == TokenKind::Error || n.kind() == TokenKind::ParserError)
}

// --- bus definitions --------------------------------------------------------------------------

#[test]
fn bus_def_exposes_name_params_and_body() {
    let src = "pragma circom 2.0.0;\nbus Point(n) { signal input x; }";
    let prog = program(src);
    let buses = prog.bus_list();
    assert_eq!(buses.len(), 1, "program must contain one bus");

    let bus = &buses[0];
    // Name value: "Point".
    assert_eq!(
        bus.identifier().unwrap().syntax().text().to_string(),
        "Point"
    );
    // Parameters: ["n"].
    let params: Vec<String> = bus
        .parameter_list()
        .expect("bus has a parameter list")
        .parameters()
        .into_iter()
        .map(|p| p.syntax().text().to_string())
        .collect();
    assert_eq!(params, vec!["n".to_string()]);
    // Body block is present and contains statements.
    assert!(bus.statements().is_some());
}

#[test]
fn bus_def_without_params_has_empty_parameter_list() {
    let src = "pragma circom 2.0.0;\nbus B { signal input x; }";
    let prog = program(src);
    let bus = &prog.bus_list()[0];
    assert_eq!(bus.identifier().unwrap().syntax().text().to_string(), "B");
    // Params are optional but the ParameterList node always exists; it just has no parameters.
    assert!(bus.parameter_list().is_some());
    assert!(bus.parameter_list().unwrap().parameters().is_empty());
}

// --- inline array / tuple / parallel expressions ---------------------------------------------

#[test]
fn inline_array_elements_are_the_values() {
    let src = "template T() { var arr = [a, b, c]; }";
    let root = syntax_tree(src);
    let arr = first::<AstInlineArray>(&root);
    let elems: Vec<String> = arr
        .elements()
        .into_iter()
        .map(|e| e.text().to_string().trim().to_string())
        .collect();
    assert_eq!(
        elems,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

#[test]
fn tuple_expression_elements_are_the_values() {
    // The first element is wrapped to match the rest, so `elements()` returns ALL of them.
    let src = "template T() { var (x, y) = (a, b); }";
    let root = syntax_tree(src);
    let tuple = first::<AstTupleExpr>(&root);
    let elems: Vec<String> = tuple
        .elements()
        .into_iter()
        .map(|e| e.text().to_string().trim().to_string())
        .collect();
    assert_eq!(elems, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(tuple.elements().len(), 2);
}

#[test]
fn parallel_expression_operand_is_accessible() {
    let src = "template T() { signal output o; o <== parallel acc; }";
    let root = syntax_tree(src);
    let par = first::<AstParallelExpr>(&root);
    let operand = par.operand().expect("parallel wraps an operand Expression");
    assert_eq!(operand.text().to_string().trim(), "acc");
}

// --- existing AST accessors are not regressed by the grammar refactor --------------------------

#[test]
fn template_accessors_survive_definition_body_extraction() {
    // definition_body now backs template/function/bus; verify template name/params/body still work.
    let src = "template Multiplier(n, m) { signal input a; }";
    let root = syntax_tree(src);
    let prog = AstCircomProgram::cast(root).unwrap();
    let tmpl = &prog.template_list()[0];
    assert_eq!(
        tmpl.identifier().unwrap().syntax().text().to_string(),
        "Multiplier"
    );
    let params: Vec<String> = tmpl
        .parameter_list()
        .unwrap()
        .parameters()
        .into_iter()
        .map(|p| p.syntax().text().to_string())
        .collect();
    assert_eq!(params, vec!["n".to_string(), "m".to_string()]);
    assert!(tmpl.body().is_some());
}

#[test]
fn realistic_program_has_no_errors_and_exposes_constructs() {
    // End-to-end: every new feature is reachable through the typed AST.
    let src = r#"pragma circom 2.0.0;
pragma custom_templates;
bus Inputs(n) { signal input a[n]; }
template custom Poseidon(n) { signal output o; o <== parallel state; }
template Aggregator(n) { input Inputs(n) inp; var proof = [a, b]; var pair = (a, b); }
component main = Aggregator(4);
"#;
    let root = syntax_tree(src);
    assert!(!has_error(&root), "realistic program must be error-free");
    let prog = AstCircomProgram::cast(root).unwrap();
    assert_eq!(prog.bus_list().len(), 1);
    assert_eq!(prog.template_list().len(), 2);
    // The parallel expression and inline array / tuple are present in the tree.
    assert!(prog
        .syntax()
        .descendants()
        .any(|n| n.kind() == TokenKind::ParallelKw));
    assert!(prog
        .syntax()
        .descendants()
        .any(|n| n.kind() == TokenKind::InlineArray));
    assert!(prog
        .syntax()
        .descendants()
        .any(|n| n.kind() == TokenKind::TupleExpr));
}
