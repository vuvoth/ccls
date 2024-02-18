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
    #[test]
    fn pragam_test() {
        use crate::{
            ast::{AstNode, AstPragma},
            syntax_node::SyntaxNode,
            token_kind::TokenKind,
        };

        use super::{entry::Scope, Parser};

        let source: String = r#"pragma circom 2.0.1;"#.to_string();

        let green_node = Parser::parse_scope(&source, Scope::Pragma);
        let node = SyntaxNode::new_root(green_node);

        let pragma = AstPragma::cast(node.last_child().unwrap()).unwrap();

        assert!(pragma.version().unwrap().syntax().kind() == TokenKind::Version);
    }
}
