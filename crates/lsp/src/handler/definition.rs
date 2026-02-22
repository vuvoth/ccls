//! Go-to-definition entry point
//!
//! Public API for the go-to-definition feature.

use lsp_types::{Location, Position};
use parser::Token;
use syntax::abstract_syntax_tree::Program;
use syntax::syntax_node::SyntaxKind;

use crate::database::{FileDB, FileSemantics};

use super::{resolve, resolve_include, token_at};

/// Main entry point for go-to-definition
pub fn goto_definition(
    file: &FileDB,
    program: &Program,
    semantics: &FileSemantics,
    pos: Position,
) -> Vec<Location> {
    let Some(token) = token_at(file, program, pos) else {
        return Vec::new();
    };

    // String tokens are include paths
    if token.kind() == SyntaxKind::from_token(Token::String) {
        return resolve_include(file, &token);
    }

    resolve(file, program, semantics, &token)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{Position, Url};
    use rowan::ast::AstNode;
    use syntax::{abstract_syntax_tree::Program, syntax::SyntaxTreeBuilder};

    use super::goto_definition;
    use crate::database::{FileDB, SemanticDB};

    fn setup_definition_test(source: &str) -> (FileDB, Program, SemanticDB) {
        let url = Url::from_file_path(Path::new("/test.circom")).unwrap();
        let file = FileDB::new(source, url);
        let syntax = SyntaxTreeBuilder::syntax_tree(source);
        let program = Program::cast(syntax).expect("Failed to parse program");
        let mut db = SemanticDB::default();
        db.index_program(&file, &program);
        (file, program, db)
    }

    fn get_definitions(
        file: &FileDB,
        program: &Program,
        db: &SemanticDB,
        line: u32,
        col: u32,
    ) -> Vec<lsp_types::Location> {
        let semantics = db.get(file.id).unwrap();
        goto_definition(file, program, semantics, Position::new(line, col))
    }

    #[test]
    fn test_goto_definition_template_ref() {
        let source = r#"template Multiplier() {
    signal output c;
}
template Main() {
    component m = Multiplier();
}"#;
        let (file, program, db) = setup_definition_test(source);
        // "Multiplier" at line 4, column 18 (0-indexed)
        let defs = get_definitions(&file, &program, &db, 4, 18);
        assert!(!defs.is_empty(), "Should find Multiplier template");
    }

    #[test]
    fn test_goto_definition_signal_in_expression() {
        let source = r#"template Multiplier() {
    signal input a;
    signal output c;
    c <== a * 2;
}"#;
        let (file, program, db) = setup_definition_test(source);
        // "a" at line 3, column 10 (0-indexed)
        let defs = get_definitions(&file, &program, &db, 3, 10);
        assert!(!defs.is_empty(), "Should find signal 'a' definition");
    }

    #[test]
    fn test_goto_definition_component_name() {
        let source = r#"template Adder() {
    signal output c;
}
template Main() {
    component myAdder = Adder();
    myAdder.c <== 5;
}"#;
        let (file, program, db) = setup_definition_test(source);
        // "myAdder" at line 5, column 4 (0-indexed)
        let defs = get_definitions(&file, &program, &db, 5, 4);
        assert!(
            !defs.is_empty(),
            "Should find component 'myAdder' definition"
        );
    }

    #[test]
    fn test_goto_definition_include_path() {
        let source = r#"include "./lib.circom";"#;
        let (file, program, db) = setup_definition_test(source);
        // Position at the string literal "./lib.circom"
        let defs = get_definitions(&file, &program, &db, 0, 10);
        assert!(!defs.is_empty(), "Should resolve include path");
        assert!(defs[0].uri.path().ends_with("lib.circom"));
    }

    #[test]
    fn test_goto_definition_no_token_returns_empty() {
        let source = r#"template Main() {
    signal output c;
}"#;
        let (file, program, db) = setup_definition_test(source);
        // Position at the end (no token)
        let defs = get_definitions(&file, &program, &db, 0, 0);
        assert!(defs.is_empty(), "Should return empty when no token found");
    }

    #[test]
    fn test_goto_definition_unknown_symbol_returns_empty() {
        let source = r#"template Main() {
    signal output c;
    c <== unknownSymbol;
}"#;
        let (file, program, db) = setup_definition_test(source);
        // "unknownSymbol" at line 2, column 9 (0-indexed)
        let defs = get_definitions(&file, &program, &db, 2, 9);
        assert!(defs.is_empty(), "Should return empty for unknown symbol");
    }

    #[test]
    fn test_goto_definition_function_call() {
        let source = r#"function helper(x) {
    return x * 2;
}
template Main() {
    var y = helper(5);
}"#;
        let (file, program, db) = setup_definition_test(source);
        // "helper" at line 4, column 12 (0-indexed)
        let defs = get_definitions(&file, &program, &db, 4, 12);
        assert!(!defs.is_empty(), "Should find function 'helper'");
    }

    #[test]
    fn test_goto_definition_param_in_template() {
        let source = r#"template Multiplier(N) {
    signal input in[N];
    var size = N;
}"#;
        let (file, program, db) = setup_definition_test(source);
        // "N" at line 2, column 15 (0-indexed)
        // Line 2: "    var size = N;"
        // Col:     0123456789012345
        let defs = get_definitions(&file, &program, &db, 2, 15);
        assert!(!defs.is_empty(), "Should find param 'N' definition");
    }
}
