use lsp_types::Location;
use lsp_types::Position;
use lsp_types::Range;
use lsp_types::Url;
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use rowan::SyntaxText;

use syntax::abstract_syntax_tree::AstComponentCall;
use syntax::abstract_syntax_tree::AstInclude;
use syntax::abstract_syntax_tree::AstTemplateDef;
use syntax::abstract_syntax_tree::AstTemplateName;
use syntax::abstract_syntax_tree::{AstCircomProgram, AstComponentDecl};
use syntax::syntax_node::SyntaxNode;
use syntax::syntax_node::SyntaxToken;

use crate::database::{FileDB, SemanticData, TokenId};

// find the first ancestor with given kind of a syntax token
pub fn lookup_node_wrap_token(ast_type: TokenKind, token: &SyntaxToken) -> Option<SyntaxNode> {
    let mut p = token.parent();
    while let Some(t) = p {
        if t.kind() == ast_type {
            return Some(t);
        }
        p = t.parent();
    }
    None
}

// return an Identifier/CircomString token at a position
pub fn lookup_token_at_postion(
    file: &FileDB,
    ast: &AstCircomProgram,
    position: Position,
) -> Option<SyntaxToken> {
    let off_set = file.off_set(position);
    ast.syntax().token_at_offset(off_set).find_map(|token| {
        let kind = token.kind();

        if kind == TokenKind::Identifier {
            return Some(token);
        }

        if kind == TokenKind::CircomString {
            return Some(token);
        }
        None
    })
}

// find all template name (in component declaration) which are used inside a template
pub fn lookup_component(template: &AstTemplateDef, text: SyntaxText) -> Option<AstTemplateName> {
    if let Some(statements) = template.statements() {
        for component in statements.find_children::<AstComponentDecl>() {
            if let Some(iden) = component.component_identifier() {
                if iden.name().unwrap().syntax().text() == text {
                    return component.template();
                }
            }
        }
    }
    None
}

// if token in an include statement
// add lib path (location of source code of that library) into result
pub fn jump_to_lib(file: &FileDB, token: &SyntaxToken) -> Vec<Location> {
    if let Some(include_lib) = lookup_node_wrap_token(TokenKind::IncludeKw, token) {
        if let Some(ast_include) = AstInclude::cast(include_lib) {
            if let Some(abs_lib_ans) = ast_include.lib() {
                let lib_path = file
                    .get_path()
                    .parent()
                    .unwrap()
                    .join(abs_lib_ans.value())
                    .clone();
                let lib_url = Url::from_file_path(lib_path.clone()).unwrap();
                return vec![Location::new(lib_url, Range::default())];
            }
        }
    }

    Vec::new()
}

pub fn lookup_definition(
    file: &FileDB,
    ast: &AstCircomProgram,
    semantic_data: &SemanticData,
    token: &SyntaxToken,
) -> Vec<Location> {
    let template_list = ast.template_list();
    // TODO: extract function list

    let mut res = Vec::new();

    if token.kind() == TokenKind::CircomString {
        return jump_to_lib(file, token);
    }

    // signal from other template
    // eg: in1, in2 from component call mul(in1, in2)
    let mut signal_outside = false;

    if let Some(component_call) = lookup_node_wrap_token(TokenKind::ComponentCall, token) {
        // find template called.
        if let Some(ast_component_call) = AstComponentCall::cast(component_call) {
            if let Some(signal) = ast_component_call.signal() {
                // if target token is the parameter of a component call
                // TODO: go to params in template!!! (failed)
                if signal.syntax().text() == token.text() {
                    signal_outside = true;
                    // lookup template of component
                    if let Some(current_template) =
                        lookup_node_wrap_token(TokenKind::TemplateDef, token)
                    {
                        if let Some(ast_template_name) = lookup_component(
                            &AstTemplateDef::cast(current_template).unwrap(),
                            ast_component_call.component_name().unwrap().syntax().text(),
                        ) {
                            if let Some(other_template) =
                                ast.get_template_by_name(&ast_template_name)
                            {
                                let template_id = other_template.syntax().token_id();
                                if let Some(semantic) =
                                    semantic_data.template_data_semantic.get(&template_id)
                                {
                                    if let Some(tmp) =
                                        semantic.signal.0.get(&signal.syntax().token_id())
                                    {
                                        res.extend(tmp)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !signal_outside {
        // look up token in template information
        // (template name, signal/variable/component in template)
        for template in template_list {
            let template_name = template.name().unwrap();
            if template_name.name().unwrap().syntax().text() == token.text() {
                let range = file.range(template.syntax());
                res.push(range);
            }

            if !template
                .syntax()
                .text_range()
                .contains_range(token.text_range())
            {
                continue;
            }

            let template_id = template.syntax().token_id();

            if let Some(data) = semantic_data.lookup_signal(template_id, token) {
                res.extend(data);
            }

            if let Some(data) = semantic_data.lookup_variable(template_id, token) {
                res.extend(data);
            }

            if let Some(component_decl) = semantic_data.lookup_component(template_id, token) {
                res.extend(component_decl);
            }
        }

        // TODO: look up token in function information
        // (function name, signal/variable/component in function)
    }

    res.into_iter()
        .map(|range| Location::new(file.file_path.clone(), range))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::Url;
    use parser::token_kind::TokenKind;
    use rowan::ast::AstNode;
    use syntax::{
        abstract_syntax_tree::{AstCircomProgram, AstInputSignalDecl},
        syntax::SyntaxTreeBuilder,
    };

    use crate::{database::FileDB, handler::goto_definition::lookup_node_wrap_token};

    use super::lookup_token_at_postion;

    fn get_source_from_path(file_path: &str) -> String {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let full_path = format!("{}{}", crate_path, file_path);
        let source = std::fs::read_to_string(&full_path).expect(&full_path);

        source
    }

    #[test]
    fn goto_decl_test() {
        let file_path = "/src/test_files/handler/templates.circom";
        let source = get_source_from_path(file_path);
        let file = FileDB::create(&source, Url::from_file_path(Path::new("/tmp")).unwrap());

        let syntax_node = SyntaxTreeBuilder::syntax_tree(&source);

        if let Some(program_ast) = AstCircomProgram::cast(syntax_node) {
            let inputs = program_ast.template_list()[0]
                .func_body()
                .unwrap()
                .statement_list()
                .unwrap()
                .find_children::<AstInputSignalDecl>();
            let signal_name = inputs[0].signal_identifier().unwrap().name().unwrap();

            let tmp = signal_name.syntax().text_range().start();

            if let Some(token) = lookup_token_at_postion(&file, &program_ast, file.position(tmp)) {
                let wrap_token = lookup_node_wrap_token(TokenKind::TemplateDef, &token);

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
        let url = Url::from_file_path(Path::new("/hello/abc.tx"));
        let binding = url.unwrap();
        let path = binding.path();
        let parent = Path::new(path).parent().unwrap().to_str().unwrap();

        assert_eq!("/hello", parent);
    }
}
