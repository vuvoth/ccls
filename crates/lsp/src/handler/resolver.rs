//! Symbol resolution
//!
//! Finds the definition location for a given symbol.

use lsp_types::{Location, Range, Url};
use parser::Rule;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::{Function, Include, Program, Template};
use syntax::syntax_node::{SyntaxKind, SyntaxToken};

use crate::database::{FileDB, FileSemantics};

use super::{location, Context};

/// Resolve a token to its definition location(s)
pub fn resolve(
    file: &FileDB,
    program: &Program,
    semantics: &FileSemantics,
    token: &SyntaxToken,
) -> Vec<Location> {
    match Context::analyze(token) {
        Context::TemplateRef => resolve_template(program, file, token),
        Context::InTemplate(tmpl) => resolve_in_template(file, program, semantics, token, &tmpl),
        Context::InFunction(func) => resolve_in_function(file, program, semantics, token, &func),
        Context::Unknown => resolve_globally(program, file, token),
    }
}

/// Resolve a template reference to its definition
fn resolve_template(program: &Program, file: &FileDB, token: &SyntaxToken) -> Vec<Location> {
    program
        .find_template(token.text())
        .map(|t| vec![location(file, t.syntax_node())])
        .unwrap_or_default()
}

/// Resolve a symbol inside a template scope
fn resolve_in_template(
    file: &FileDB,
    program: &Program,
    semantics: &FileSemantics,
    token: &SyntaxToken,
    template: &Template,
) -> Vec<Location> {
    // Look up in local scope
    if let Some(name) = template.name() {
        if let Some(def) = semantics.lookup_in_template(name.text(), token.text()) {
            return vec![Location::new(file.url.clone(), def.range)];
        }
    }

    // Fall back to global lookup
    resolve_globally(program, file, token)
}

/// Resolve a symbol inside a function scope
fn resolve_in_function(
    file: &FileDB,
    program: &Program,
    semantics: &FileSemantics,
    token: &SyntaxToken,
    func: &Function,
) -> Vec<Location> {
    if let Some(name) = func.name() {
        if let Some(def) = semantics.lookup_in_function(name.text(), token.text()) {
            return vec![Location::new(file.url.clone(), def.range)];
        }
    }

    resolve_globally(program, file, token)
}

/// Global symbol lookup (templates and functions)
fn resolve_globally(program: &Program, file: &FileDB, token: &SyntaxToken) -> Vec<Location> {
    let mut results = Vec::new();

    if let Some(t) = program.find_template(token.text()) {
        results.push(location(file, t.syntax_node()));
    }

    if let Some(f) = program.find_function(token.text()) {
        results.push(location(file, f.syntax_node()));
    }

    results
}

/// Resolve an include path to a file location
pub fn resolve_include(file: &FileDB, token: &SyntaxToken) -> Vec<Location> {
    let include = token
        .parent_ancestors()
        .find(|n| n.kind() == SyntaxKind::from_rule(Rule::Include))
        .and_then(Include::cast);

    let Some(include) = include else {
        return Vec::new();
    };
    let Some(path) = include.path() else {
        return Vec::new();
    };

    file.path()
        .parent()
        .and_then(|p| Url::from_file_path(p.join(path)).ok())
        .map(|u| vec![Location::new(u, Range::default())])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{Position, Url};
    use rowan::ast::AstNode;
    use syntax::{abstract_syntax_tree::Program, syntax::SyntaxTreeBuilder};

    use super::resolve;
    use crate::database::{FileDB, SemanticDB};

    fn setup_resolver_test(source: &str) -> (FileDB, Program, SemanticDB) {
        let url = Url::from_file_path(Path::new("/test.circom")).unwrap();
        let file = FileDB::new(source, url);
        let syntax = SyntaxTreeBuilder::syntax_tree(source);
        let program = Program::cast(syntax).expect("Failed to parse program");
        let mut db = SemanticDB::default();
        db.index_program(&file, &program);
        (file, program, db)
    }

    fn get_definitions_at(
        file: &FileDB,
        program: &Program,
        db: &SemanticDB,
        line: u32,
        col: u32,
    ) -> Vec<lsp_types::Location> {
        use crate::handler::token_at;
        let semantics = db.get(file.id).unwrap();
        let pos = Position::new(line, col);
        let Some(token) = token_at(file, program, pos) else {
            return Vec::new();
        };
        resolve(file, program, semantics, &token)
    }

    #[test]
    fn test_resolve_template_from_component_decl() {
        let source = r#"template Multiplier() {
    signal output c;
}
template Main() {
    component m = Multiplier();
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 4, 18);
        assert!(!defs.is_empty(), "Should find template definition");
        assert!(defs[0].uri.path().ends_with("test.circom"));
    }

    #[test]
    fn test_resolve_template_from_template_call() {
        let source = r#"template Adder() {
    signal output c;
}
template Main() {
    signal x;
    x <== Adder()().c;
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 5, 10);
        assert!(!defs.is_empty(), "Should find Adder template");
    }

    #[test]
    fn test_resolve_signal_to_declaration() {
        let source = r#"template Multiplier() {
    signal input a;
    signal output c;
    c <== a * 2;
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 3, 10);
        assert!(!defs.is_empty(), "Should find signal 'a' declaration");
    }

    #[test]
    fn test_resolve_component_to_declaration() {
        let source = r#"template Adder() {
    signal output c;
}
template Main() {
    component myAdder = Adder();
    myAdder.c <== 5;
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 5, 4);
        assert!(
            !defs.is_empty(),
            "Should find component 'myAdder' declaration"
        );
    }

    #[test]
    fn test_resolve_param_to_declaration() {
        let source = r#"template Multiplier(N) {
    signal input in[N];
    signal output out;
    var size = N;
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 3, 15);
        assert!(!defs.is_empty(), "Should find param 'N' declaration");
    }

    #[test]
    fn test_resolve_var_to_declaration() {
        let source = r#"template Counter() {
    var count = 0;
    count = count + 1;
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 2, 4);
        assert!(!defs.is_empty(), "Should find var 'count' declaration");
    }

    #[test]
    fn test_resolve_function_globally() {
        let source = r#"function helper(x) {
    return x * 2;
}
template Main() {
    var y = helper(5);
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 4, 12);
        assert!(!defs.is_empty(), "Should find function 'helper'");
    }

    #[test]
    fn test_resolve_unknown_returns_empty() {
        let source = r#"template Main() {
    signal output c;
    c <== unknownSymbol;
}"#;
        let (file, program, db) = setup_resolver_test(source);
        let defs = get_definitions_at(&file, &program, &db, 2, 9);
        assert!(defs.is_empty(), "Should return empty for unknown symbol");
    }
}
