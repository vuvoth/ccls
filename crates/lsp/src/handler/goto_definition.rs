use lsp_types::Location;
use lsp_types::{Position, Range};
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use rowan::SyntaxText;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::abstract_syntax_tree::AstComponentCall;
use syntax::abstract_syntax_tree::AstTemplateDef;
use syntax::abstract_syntax_tree::AstTemplateName;
use syntax::syntax_node::SyntaxNode;
use syntax::syntax_node::SyntaxToken;

use super::lsp_utils::FileUtils;

/**
 * api: lookup template have name,
 */

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

pub fn lookup_token_at_postion(
    file: &FileUtils,
    ast: &AstCircomProgram,
    position: Position,
) -> Option<SyntaxToken> {
    let off_set = file.off_set(position);
    ast.syntax().token_at_offset(off_set).find_map(|token| {
        let kind = token.kind();

        if kind == TokenKind::Identifier {
            return Some(token);
        }

        None
    })
}

pub fn lookup_component(template: &AstTemplateDef, text: SyntaxText) -> Option<AstTemplateName> {
    if let Some(statements) = template.statements() {
        for component in statements.components() {
            if let Some(iden) = component.component_identifier() {
                if iden.name().unwrap().syntax().text() == text {
                    return component.template();
                }
            }
        }
    }
    None
}

pub fn lookup_definition(
    file: &FileUtils,
    ast: &AstCircomProgram,
    token: &SyntaxToken,
) -> Vec<Location> {
    eprintln!("{token:?}");
    let template_list = ast.template_list();

    let mut res = Vec::new();
    let mut signal_outside = false;
    if let Some(component_call) = lookup_node_wrap_token(TokenKind::ComponentCall, token) {
        // find template called.
        if let Some(ast_component_call) = AstComponentCall::cast(component_call) {
            if let Some(signal) = ast_component_call.signal() {
                if signal.syntax().text() == token.text() {
                    signal_outside = true;
                    // lookup template of componenet
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
                                if let Some(inp) =
                                    other_template.find_input_signal(&signal.syntax().text())
                                {
                                    res.push(file.range(inp.syntax()));
                                }
                                if let Some(out) =
                                    other_template.find_output_signal(&signal.syntax().text())
                                {
                                    res.push(file.range(out.syntax()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !signal_outside {
        for template in template_list {
            let template_name = template.template_name().unwrap();
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

            res.extend(lookup_signal_in_template(file, &template, token).into_iter());

            if let Some(component_decl) = template.find_component(token.text()) {
                res.push(file.range(component_decl.syntax()));
            }

            if let Some(fn_body) = template.func_body() {
                if let Some(statements) = fn_body.statement_list() {
                    for var in statements.variables() {
                        if let Some(name) = var.variable_name() {
                            if name.syntax().text() == token.text() {
                                res.push(file.range(var.syntax()));
                            }
                        }
                    }
                }
            }
        }
    }

    res.into_iter()
        .map(|range| Location::new(file.file_path.clone(), range))
        .collect()
}

fn lookup_signal_in_template(
    file: &FileUtils,
    ast_template: &AstTemplateDef,
    signal_token: &SyntaxToken,
) -> Vec<Range> {
    let mut result = Vec::new();
    if let Some(block) = ast_template.func_body() {
        if let Some(statements) = block.statement_list() {
            for signal in statements.input_signals() {
                if let Some(name) = signal.signal_name() {
                    if name.syntax().text() == signal_token.text() {
                        result.push(file.range(signal.syntax()));
                    }
                }
            }

            for signal in statements.output_signals() {
                if let Some(name) = signal.signal_name() {
                    if name.syntax().text() == signal_token.text() {
                        result.push(file.range(signal.syntax()));
                    }
                }
            }

            for signal in statements.internal_signals() {
                if let Some(name) = signal.signal_name() {
                    if name.syntax().text() == signal_token.text() {
                        result.push(file.range(signal.syntax()));
                    }
                }
            }
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::Url;
    use parser::token_kind::TokenKind;
    use rowan::ast::AstNode;
    use syntax::{abstract_syntax_tree::AstCircomProgram, syntax::SyntaxTreeBuilder};

    use crate::handler::{
        goto_definition::{lookup_node_wrap_token, lookup_token_at_postion},
        lsp_utils::FileUtils,
    };

    #[test]
    fn goto_decl_test() {
        let source: String = r#"
        pragma circom 2.0.0;

        template X() {
            signal x = 10;
            signal input x = 10;
           component x = Multiplier2();
           component y = X();
           component y = Multiplier2();
           component z = Multiplier2();
              
        }
template M() {
           component h = X();
           component k = Multiplier2(); 
           test
        }
template Multiplier2 () {  
           template m = M();
           // hello world
           signal input a;  
           signal input b;  
              signal output c;  
           component y = X();
           
           mintlkrekerjke;
           component e = Y();
           component z = Y();
           component h = Y();
           signal output d;
           c <== a * b; 
        }
template Y() {
           component y = X();
           component a = X();
           
        }        
        "#
        .to_string();

        let file = FileUtils::create(&source, Url::from_file_path(Path::new("tmp")).unwrap());

        let syntax_node = SyntaxTreeBuilder::syntax_tree(&source);

        if let Some(program_ast) = AstCircomProgram::cast(syntax_node) {
            for template in program_ast.template_list() {
                println!("{template:?}");
            }

            let inputs = program_ast.template_list()[0]
                .func_body()
                .unwrap()
                .statement_list()
                .unwrap()
                .input_signals();
            let signal_name = inputs[0].signal_name().unwrap();

            let tmp = signal_name.syntax().text_range().start();

            if let Some(token) = lookup_token_at_postion(&file, &program_ast, file.position(tmp)) {
                println!(
                    "{:#?}",
                    lookup_node_wrap_token(TokenKind::TemplateDef, &token)
                );
            }
        }
    }

    #[test]
    fn url_test() {
        let url = Url::from_file_path(Path::new("/hello/abc.tx"));
        let binding = url.unwrap();
        let p = binding.path();
        println!("{:?}", Path::new(p).parent());
    }
}
