use lsp_types::{Position, Range};
use parser::{
    ast::{AstCircomProgram, AstNode},
    syntax_node::SyntaxToken,
    token_kind::TokenKind,
};

use super::lsp_utils::FileUtils;

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

pub fn lookup_definition(
    file: &FileUtils,
    ast: &AstCircomProgram,
    token: SyntaxToken,
) -> Vec<Range> {
    let template_list = ast.template_list();

    let mut result = Vec::new();

    for template in template_list {
        let template_name = template.template_name().unwrap();
        if template_name.name().unwrap().syntax().text() == token.text() {
            let range = file.range(template.syntax());
            result.push(range);
        }

        if !template.syntax().text_range().contains_range(token.text_range()) {
            break;
        }
        
        if let Some(fn_body) = template.func_body() {
            if let Some(statements) = fn_body.statement_list() {
                for signal in statements.input_signals() {
                    if let Some(name) = signal.signal_name() {
                        if name.syntax().text() == token.text() {
                            result.push(file.range(signal.syntax()));
                        }
                    }
                }

               
                for signal in statements.output_signals() {
                    if let Some(name) = signal.signal_name() {
                        if name.syntax().text() == token.text() {
                            result.push(file.range(signal.syntax()));
                        }
                    }
                }

                for signal in statements.internal_signals() {
                    if let Some(name) = signal.signal_name() {
                        if name.syntax().text() == token.text() {
                            result.push(file.range(signal.syntax()));
                        }
                    }
                }

            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use parser::{ast::AstCircomProgram, parser::Parser, syntax_node::SyntaxNode};
    use rowan::{ast::AstNode, TextSize};

    use crate::handler::lsp_utils::FileUtils;

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

        let file = FileUtils::create(&source);

        let green_node = Parser::parse_circom(&source);
        let syntax_node = SyntaxNode::new_root(green_node.clone());
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

            println!("{:?}", program_ast.syntax().token_at_offset(tmp));
        }
    }
}
