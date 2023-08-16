use super::{expression::expression, *};

pub fn block(p: &mut Parser) {
    if !p.at(LCurly) {
        p.advance_with_error("Miss {");
    } else {
        let m = p.open();
        p.eat(LCurly);
        while !p.at(RCurly) && !p.eof() {
            let kind = p.current().kind;
            match kind {
                SignalKw => {
                    declaration::signal_declaration(p);
                    p.expect(Semicolon);
                }
                VarKw => {
                    declaration::var_declaration(p);
                    p.expect(Semicolon);
                }
                _ => statement::statement(p),
            }
        }

        p.expect(RCurly);

        p.close(m, Block);
    }
}

#[cfg(test)]
mod tests {
    use logos::Lexer;

    use crate::{grammar::entry::Scope, token_kind::TokenKind};

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
        let mut lexer = Lexer::<TokenKind>::new(source);
        let mut parser = Parser::new(&mut lexer);

        parser.parse(Scope::Block);

        let cst = parser.build_tree();

        println!("{:?}", cst);
    }
}
