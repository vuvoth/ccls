use crate::grammar::*;
/**
 * template Identifier() {content}
 *
 */
pub fn template(p: &mut Parser) {
    // assert!(p.at(TemplateKw));
    let m = p.open();
    p.expect(TemplateKw);
    p.expect(Identifier);
    p.expect(LParen);
    p.expect(RParen);
    block::block(p);
    p.close(m, TemplateKw);
}

pub fn function_parse(p: &mut Parser) {
    let m = p.open();
    p.expect(FunctionKw);
    p.expect(Identifier);
    p.expect(LParen);
    p.expect(RParen);
    block::block(p);
    p.close(m, FunctionDef);
}

mod tests {
    use crate::ast::TemplateDef;

    #[test]
    fn template_parse_test() {
        use crate::{
            ast::{AstNode, PragmaDef},
            syntax_node::SyntaxNode,
            token_kind::TokenKind,
        };

        use super::{entry::Scope, Parser};

        let source: String = r#"
        template Multiplier2 () {  
        
           // Declaration of signals.  
           signal input a;  
           signal input b;  
           signal output c;  
        
           // Constraints.  
           c <== a * b;  
        }

        "#
        .to_string();

        let green_node = Parser::parse_scope(&source, Scope::Template);
        let node = SyntaxNode::new_root(green_node);

        let ast_template = TemplateDef::cast(node);

        if let Some(ast_internal) = ast_template {
            println!(
                "name {:?}",
                ast_internal.func_name().unwrap().syntax().text()
            );
        }
    }
}
