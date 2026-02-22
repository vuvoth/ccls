//! Context analysis for symbol resolution
//!
//! Determines what kind of symbol the cursor is on by walking the AST.

use parser::Rule;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::{Function, Template, TemplateCall};
use syntax::syntax_node::{SyntaxKind, SyntaxToken};

/// Context of where a token appears in the AST
#[derive(Debug, Clone)]
pub enum Context {
    /// Unknown context, fall back to global search
    Unknown,
    /// Token is a template name in a template call
    TemplateRef,
    /// Token is inside a template body
    InTemplate(Template),
    /// Token is inside a function body
    InFunction(Function),
}

impl Context {
    /// Analyze the context around a token by walking up the AST
    pub fn analyze(token: &SyntaxToken) -> Self {
        let token_text = token.text();

        for ancestor in token.parent_ancestors() {
            // Check if we're in a template call (e.g., `Multiplier()`)
            if ancestor.kind() == SyntaxKind::from_rule(Rule::TemplateCall) {
                if let Some(call) = TemplateCall::cast(ancestor.clone()) {
                    if let Some(name) = call.template_name() {
                        if name.text() == token_text {
                            return Context::TemplateRef;
                        }
                    }
                }
            }

            // Check if we're inside a template
            if ancestor.kind() == SyntaxKind::from_rule(Rule::Template) {
                if let Some(template) = Template::cast(ancestor.clone()) {
                    // Skip if this IS the template's name (declaration)
                    if template
                        .name()
                        .map(|n| n.text() == token_text)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    return Context::InTemplate(template);
                }
            }

            // Check if we're inside a function
            if ancestor.kind() == SyntaxKind::from_rule(Rule::Function) {
                if let Some(func) = Function::cast(ancestor) {
                    if func.name().map(|n| n.text() == token_text).unwrap_or(false) {
                        continue;
                    }
                    return Context::InFunction(func);
                }
            }
        }

        Context::Unknown
    }
}

/// Check if a token kind represents a navigable symbol
pub fn is_symbol_token(kind: SyntaxKind) -> bool {
    use parser::Token;
    kind == SyntaxKind::from_token(Token::Identifier)
        || kind == SyntaxKind::from_token(Token::String)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::Url;
    use rowan::ast::AstNode;
    use syntax::{abstract_syntax_tree::Program, syntax::SyntaxTreeBuilder};

    use super::Context;
    use crate::database::FileDB;
    use crate::handler::token_at;

    fn setup_context_test(source: &str) -> (FileDB, Program) {
        let url = Url::from_file_path(Path::new("/test.circom")).unwrap();
        let file = FileDB::new(source, url);
        let syntax = SyntaxTreeBuilder::syntax_tree(source);
        let program = Program::cast(syntax).expect("Failed to parse program");
        (file, program)
    }

    fn get_context_at(file: &FileDB, program: &Program, line: u32, col: u32) -> Option<Context> {
        let token = token_at(file, program, lsp_types::Position::new(line, col))?;
        Some(Context::analyze(&token))
    }

    #[test]
    fn test_context_template_ref() {
        let source = r#"template Multiplier() {
    signal output c;
}
template Main() {
    component m = Multiplier();
}"#;
        let (file, program) = setup_context_test(source);
        // "Multiplier" at line 4, column 18 (in template call, 0-indexed)
        let ctx = get_context_at(&file, &program, 4, 18);
        assert!(matches!(ctx, Some(Context::TemplateRef)));
    }

    #[test]
    fn test_context_in_template() {
        let source = r#"template Multiplier() {
    signal input a;
    signal output c;
    c <== a * 2;
}"#;
        let (file, program) = setup_context_test(source);
        // "a" at line 3, column 10 (signal reference inside template, 0-indexed)
        let ctx = get_context_at(&file, &program, 3, 10);
        assert!(matches!(ctx, Some(Context::InTemplate(_))));
    }

    #[test]
    fn test_context_in_function() {
        let source = r#"function helper(x) {
    var y = x + 1;
    return y;
}"#;
        let (file, program) = setup_context_test(source);
        // "x" at line 1, column 12 (param reference inside function, 0-indexed)
        let ctx = get_context_at(&file, &program, 1, 12);
        assert!(matches!(ctx, Some(Context::InFunction(_))));
    }

    #[test]
    fn test_context_unknown() {
        let source = r#"pragma circom 2.0.0;"#;
        let (file, program) = setup_context_test(source);
        // "circom" at line 0, column 7 (0-indexed)
        let ctx = get_context_at(&file, &program, 0, 7);
        assert!(matches!(ctx, Some(Context::Unknown)) | matches!(ctx, None));
    }

    #[test]
    fn test_context_template_name_is_not_ref() {
        let source = r#"template Multiplier() {
    signal output c;
}"#;
        let (file, program) = setup_context_test(source);
        // "Multiplier" at line 0, column 9 (template declaration name, 0-indexed)
        let ctx = get_context_at(&file, &program, 0, 9);
        // The template's own name should not be treated as TemplateRef
        // It should be Unknown since it's the declaration itself
        assert!(!matches!(ctx, Some(Context::TemplateRef)));
    }
}
