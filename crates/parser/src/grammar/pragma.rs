use super::*;

/**
 * parse pragma in circom language
 * grammar:
 *      pragma circom <version>;
 */

pub fn pragma(p: &mut Parser) {
    let m = p.open();
    p.expect(Pragma);
    p.expect(Circom);
    p.expect(Version);
    p.expect(Semicolon);
    p.close(m, Pragma);
}

mod tests {

    use crate::{ast::{AstNode, PragmaDef}, syntax_node::SyntaxNode};

    use super::{entry::Scope, Parser};

    #[test]
    fn pragam_test() {
        let source: String = r#"pragma circom 2.0.1;"#.to_string();

        let green_node = Parser::parse_scope(&source, Scope::Pragma);
        let node = SyntaxNode::new_root(green_node);

        for c in node.children() {
            println!("{}", c.text());
        }

        let pragma = PragmaDef::cast(node.last_child().unwrap()).unwrap();


        
        println!("{:?}", pragma.version());
        
    }
}
