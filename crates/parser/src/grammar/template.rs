use crate::grammar::*;
/**
 * template Identifier() {content}
 *
 */
pub fn template(p: &mut Parser) {
    // assert!(p.at(TemplateKw));
    let m = p.open();
    p.expect(TemplateKw);
    let name_marker = p.open();
    p.expect(Identifier);
    p.close(name_marker, TemplateName);

    p.expect(LParen);
    let arg_marker = p.open();
    while !p.at(RParen) && !p.eof() {
        p.expect(Identifier);
        if p.at(Comma) {
            p.expect(Comma);
        }
    }

    p.close(arg_marker, ParameterList);
    p.expect(RParen);
    block::block(p);
    p.close(m, TemplateDef);
}

// #[cfg(test)]
// mod tests {
//     use crate::ast::AstTemplateDef;

//     #[test]
//     fn template_parse_test() {
//         use crate::{ast::AstNode, syntax_node::SyntaxNode};

//         use super::{entry::Scope, Parser};

//         let source: String = r#"
//         template Multiplier2 (a, b, c) {

//            // Declaration of signals.
//            signal input a;
//            signal input b;
//            signal output c;

//            // Constraints.
//            c <== a * b;
//         }

//         "#
//         .to_string();

//         let green_node = ::parse_scope(&source, Scope::Template);
//         let node = SyntaxNode::new_root(green_node);

//         let ast_template = AstTemplateDef::cast(node);

//         if let Some(ast_internal) = ast_template {
//             println!(
//                 "name {:?}",
//                 ast_internal.template_name().unwrap().syntax().text()
//             );
//         }
//     }
// }
