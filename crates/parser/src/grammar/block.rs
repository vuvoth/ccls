use super::*;

pub fn block(p: &mut Parser) {
    if !p.at(LCurly) {
        p.advance_with_error("Miss {");
    } else {
        let m = p.open();
        p.eat(LCurly);
        let stmt_marker = p.open();
        while !p.at(RCurly) && !p.eof() {
            let kind = p.current();
            match kind {
                SignalKw => {
                    declaration::signal_declaration(p);
                    p.expect(Semicolon);
                }
                VarKw => {
                    declaration::var_declaration(p);
                    p.expect(Semicolon);
                },
                ComponentKw => {
                    declaration::component_declaration(p);
                    p.expect(Semicolon);
                }
                ComponentKw => {
                    declaration::component_declaration(p);
                    p.expect(Semicolon);
                }
                _ => statement::statement(p),
            }
        }

        p.close(stmt_marker, StatementList);

        p.expect(RCurly);

        p.close(m, Block);
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        ast::{AstNode, Block},
        grammar::entry::Scope,
        syntax_node::SyntaxNode,
    };

    use super::*;
    #[test]
    fn parse_block_test() {
        let source = r#"
            {
               var x, y; 
               var (x, y);
               var (x, y) = a + b;
               var a = x, b = y;
               var a = x, b = y;
               
               signal a; 
               signal a, b;
               signal (a, b);
               signal (a, b) = a - b;
               a <== 12 + 1;
               a ==>b;
            }
        "#;
        let green_node = Parser::parse_scope(source, Scope::Block);
        let syntax_node = SyntaxNode::new_root(green_node);

        if let Some(ast_block) = Block::cast(syntax_node) {
            println!("{:?}", ast_block.statement().unwrap().syntax().kind());
        }
    }
}
