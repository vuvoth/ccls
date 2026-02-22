//! LSP request handlers

mod context;
mod definition;
mod navigation;
mod resolver;

// Public API
pub use context::{is_symbol_token, Context};
pub use definition::goto_definition;
pub use navigation::{location, token_at};
pub use resolver::{resolve, resolve_include};

// Tests
#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::Url;
    use parser::{Rule, Token};
    use rowan::ast::AstNode;
    use syntax::{
        abstract_syntax_tree::{Program, SignalDecl},
        syntax::SyntaxTreeBuilder,
        syntax_node::SyntaxKind,
    };

    use super::token_at;
    use crate::database::FileDB;

    fn get_source_from_path(file_path: &str) -> (String, std::path::PathBuf) {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let workspace_path = std::path::Path::new(&crate_path)
            .parent()
            .and_then(|p| p.parent())
            .expect("Failed to find workspace root");
        let full_path = workspace_path.join(file_path.trim_start_matches('/'));
        let source =
            std::fs::read_to_string(&full_path).expect(&format!("Failed to read {:?}", full_path));
        (source, full_path)
    }

    #[test]
    fn goto_decl_test() {
        let file_path = "tests/fixtures/lsp/handler/templates.circom";
        let (source, full_path) = get_source_from_path(file_path);
        let file = FileDB::new(&source, Url::from_file_path(&full_path).unwrap());

        let syntax_node = SyntaxTreeBuilder::syntax_tree(&source);

        if let Some(program_ast) = Program::cast(syntax_node) {
            let templates: Vec<_> = program_ast.templates().collect();
            assert!(!templates.is_empty(), "No templates found");
            let body = templates[0].body().expect("Template has no body");

            let inputs: Vec<_> = body
                .syntax()
                .children()
                .filter_map(SignalDecl::cast)
                .filter(|s| s.is_input())
                .collect();

            assert!(!inputs.is_empty(), "No input signals found");
            let signal_name = inputs[0].ident().expect("Signal has no ident");

            let tmp = signal_name.syntax_token().text_range().start();

            if let Some(token) = token_at(&file, &program_ast, file.position(tmp)) {
                let wrap_token = token
                    .parent_ancestors()
                    .find(|n| n.kind() == SyntaxKind::from_rule(Rule::Template));

                let string_syntax_node = match wrap_token {
                    None => "None".to_string(),
                    Some(syntax_node) => format!("{}", syntax_node),
                };

                insta::assert_snapshot!("test_lookup_node_wrap_token", string_syntax_node);
            }
        }
    }

    #[test]
    fn url_test() {
        let url = Url::from_file_path(Path::new("/hello/abc.tx")).unwrap();
        let path = url.path();
        let parent = Path::new(path).parent().unwrap().to_str().unwrap();

        assert_eq!("/hello", parent);
    }

    #[test]
    fn test_is_symbol_token() {
        use super::is_symbol_token;

        assert!(is_symbol_token(SyntaxKind::from_token(Token::Identifier)));
        assert!(is_symbol_token(SyntaxKind::from_token(Token::String)));
        assert!(!is_symbol_token(SyntaxKind::from_token(Token::Number)));
    }

    // Integration tests using test files
    fn setup_integration_test(file_path: &str) -> (FileDB, Program, crate::database::SemanticDB) {
        use crate::database::SemanticDB;

        let (source, full_path) = get_source_from_path(file_path);
        let url = Url::from_file_path(&full_path).unwrap();
        let file = FileDB::new(&source, url);
        let syntax = SyntaxTreeBuilder::syntax_tree(&source);
        let program = Program::cast(syntax).expect("Failed to parse program");
        let mut db = SemanticDB::default();
        db.index_program(&file, &program);
        (file, program, db)
    }

    #[test]
    fn test_goto_definition_integration_template() {
        let (file, program, db) =
            setup_integration_test("tests/fixtures/lsp/handler/goto_template.circom");
        let semantics = db.get(file.id).unwrap();

        // "Multiplier2" at line 15 (0-indexed), column 22
        let defs =
            super::goto_definition(&file, &program, semantics, lsp_types::Position::new(15, 22));
        assert!(!defs.is_empty(), "Should find Multiplier2 template");
    }

    #[test]
    fn test_goto_definition_integration_signal() {
        let (file, program, db) =
            setup_integration_test("tests/fixtures/lsp/handler/goto_signal.circom");
        let semantics = db.get(file.id).unwrap();

        // "intermediate" at line 8 (0-indexed), column 4
        let defs =
            super::goto_definition(&file, &program, semantics, lsp_types::Position::new(8, 4));
        assert!(!defs.is_empty(), "Should find intermediate signal");
    }

    #[test]
    fn test_goto_definition_integration_component() {
        let (file, program, db) =
            setup_integration_test("tests/fixtures/lsp/handler/goto_component.circom");
        let semantics = db.get(file.id).unwrap();

        // "Multiplier" at line 22 (0-indexed), column 22
        let defs =
            super::goto_definition(&file, &program, semantics, lsp_types::Position::new(22, 22));
        assert!(!defs.is_empty(), "Should find Multiplier template");

        // "mult" at line 25 (0-indexed), column 4
        let defs =
            super::goto_definition(&file, &program, semantics, lsp_types::Position::new(25, 4));
        assert!(!defs.is_empty(), "Should find mult component declaration");
    }

    #[test]
    fn test_goto_definition_integration_param() {
        let (file, program, db) =
            setup_integration_test("tests/fixtures/lsp/handler/goto_signal.circom");
        let semantics = db.get(file.id).unwrap();

        // "N" at line 14 (0-indexed), column 21
        // Line 14: "    signal input in[N];"
        let defs =
            super::goto_definition(&file, &program, semantics, lsp_types::Position::new(14, 21));
        assert!(!defs.is_empty(), "Should find N parameter");
    }

    #[test]
    fn test_cross_file_inline_template_call() {
        // Test that inline template calls in signal initialization
        // can resolve to templates in included files
        // Pattern: signal x <== Template()([args])
        use crate::database::SemanticDB;

        let main_source = r#"pragma circom 2.0.0;

include "./lib.circom";

template Main() {
    signal input y1;
    signal first_move_y <== LibMultiplier()([y1, -1]);
}"#;

        let lib_source = r#"pragma circom 2.0.0;

template LibMultiplier() {
    signal input a;
    signal input b;
    signal output c;
    c <== a * b;
}"#;

        let main_url = Url::from_file_path(Path::new("/main.circom")).unwrap();
        let lib_url = Url::from_file_path(Path::new("/lib.circom")).unwrap();

        // Set up main file
        let main_file = FileDB::new(main_source, main_url.clone());
        let main_syntax = SyntaxTreeBuilder::syntax_tree(main_source);
        let main_program = Program::cast(main_syntax.clone()).expect("Failed to parse main");

        // Debug: print lines
        for (i, line) in main_source.lines().enumerate() {
            println!("Line {}: {}", i, line);
        }

        // Set up lib file
        let lib_file = FileDB::new(lib_source, lib_url.clone());
        let lib_syntax = SyntaxTreeBuilder::syntax_tree(lib_source);
        let lib_program = Program::cast(lib_syntax.clone()).expect("Failed to parse lib");

        // Index both files
        let mut db = SemanticDB::default();
        db.index_program(&main_file, &main_program);
        db.index_program(&lib_file, &lib_program);

        // Simulate the GlobalState cross-file lookup
        let main_semantics = db.get(main_file.id).unwrap();
        let lib_semantics = db.get(lib_file.id).unwrap();

        // "LibMultiplier" - find the actual position
        // Line 6: "    signal first_move_y <== LibMultiplier()([y1, -1]);"
        //          01234567890123456789012345678901234
        //                    1111111111222222222233333
        // LibMultiplier starts at column 29
        let pos = lsp_types::Position::new(6, 29);
        println!("Looking at position: {:?}", pos);

        let token = token_at(&main_file, &main_program, pos);
        if let Some(ref t) = token {
            println!("Token found: {:?}", t.text());
        }

        if let Some(token) = token {
            assert_eq!(token.text(), "LibMultiplier");

            // Local resolution should fail (template not in main file)
            let local_results = super::resolve(&main_file, &main_program, main_semantics, &token);
            println!("Local results: {:?}", local_results);
            assert!(
                local_results.is_empty(),
                "LibMultiplier should not be found locally"
            );

            // Cross-file resolution should succeed
            let cross_results = super::resolve(&lib_file, &lib_program, lib_semantics, &token);
            println!("Cross-file results: {:?}", cross_results);
            assert!(
                !cross_results.is_empty(),
                "LibMultiplier should be found in lib file"
            );
        } else {
            // Try to find the token at different positions
            for line in 5..10 {
                for col in 25..40 {
                    let pos = lsp_types::Position::new(line, col);
                    if let Some(token) = token_at(&main_file, &main_program, pos) {
                        if token.text() == "LibMultiplier" {
                            println!("Found LibMultiplier at line {}, col {}", line, col);
                        }
                    }
                }
            }
            panic!("Token not found");
        }
    }
}
