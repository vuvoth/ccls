use super::{block::block, expression::expression, *};

pub(super) fn parse(p: &mut Parser) {}

pub(super) fn statement(p: &mut Parser) {
    let m = p.open();
    match p.current().kind {
        IfKw => if_statement(p),
        _ => statement_no_condition(p),
    }
    p.close(m, Statement);
}

fn if_statement(p: &mut Parser) {
    let m = p.open();
    p.expect(IfKw);
    p.expect(LParen);
    expression::expression(p);
    p.expect(RParen);
    statement(p);
    if p.at(ElseKw) {
        p.expect(ElseKw);
        statement(p);
    }
    p.close(m, IfKw);
}

/**
 * no if condition here.
 */
fn statement_no_condition(p: &mut Parser) {
    match p.current().kind {
        ForKw => for_statement(p),
        WhileKw => while_statement(p),
        ReturnKw => {
            return_statement(p);
            p.expect(Semicolon);
        }
        LCurly => block(p),
        LogKw => {
            log_statement(p);
            p.expect(Semicolon);
        }
        AssertKw => {
            assert_statement(p);
            p.expect(Semicolon);
        }
        _ => {
            assignment_statement(p);
            p.expect(Semicolon);
        }
    }
}

fn for_statement(p: &mut Parser) {
    let m = p.open();
    p.expect(ForKw);
    p.expect(LParen);
    if p.current().kind.is_declaration_kw() {
        declaration::declaration(p);
    } else {
        assignment_statement(p);
    }
    p.expect(Semicolon);
    expression::expression(p);
    p.expect(Semicolon);

    assignment_statement(p);
    p.expect(RParen);

    statement_no_condition(p);
    p.close(m, ForLoop);
}

fn while_statement(p: &mut Parser) {
    p.expect(WhileKw);
    p.expect(LParen);
    expression(p);
    p.expect(RParen);
    statement(p);
}

fn assert_statement(p: &mut Parser) {
    let m = p.open();
    p.expect(AssertKw);
    p.expect(LParen);
    expression(p);
    p.expect(RParen);
    p.close(m, AssertKw);
}

fn log_statement(p: &mut Parser) {
    let m = p.open();
    p.expect(LogKw);
    p.expect(LParen);
    while !p.eof() {
        if p.at(RParen) {
            break;
        }
        match p.current().kind {
            CircomString => p.advance(),
            _ => expression(p),
        }
        if !p.at(Comma) {
            break;
        } else {
            p.advance();
        }
    }
    p.expect(RParen);
    p.close(m, LogKw);
}

fn return_statement(p: &mut Parser) {
    let m = p.open();
    p.expect(ReturnKw);
    expression(p);
    p.close(m, ReturnKw);
}

fn assignment_statement(p: &mut Parser) {
    let m = p.open();
    expression(p);
    p.expect_any(&[
        Assign,
        RAssignSignal,
        RAssignConstraintSignal,
        LAssignContraintSignal,
        LAssignSignal,
        EqualSignal,
    ]);
    expression(p);
    p.close(m, AssignStatement);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_kind::TokenKind;
    use logos::Lexer;

    #[test]
    fn if_statement_test() {
        let source = r#"
            assert(1 == 2);
        "#;
        let mut lexer = Lexer::<TokenKind>::new(source);
        let mut parser = Parser::new(&mut lexer);

        println!("{}", source);

        statement(&mut parser);
        let cst = parser.build_tree();

        println!("{:?}", cst);
    }
}
