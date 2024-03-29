use super::{block::block, expression::expression, *};

pub(super) fn statement(p: &mut Parser) {
    let m = p.open();
    match p.current() {
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
    match p.current() {
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
    if p.current().is_declaration_kw() {
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
        match p.current() {
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

    if p.at(Identifier) {
        let m_id = p.open();
        let m_name = p.open();
        p.expect(Identifier);
        p.close(m_name, ComponentIdentifier);
        if p.at(LBracket) {
            p.expect(LBracket);
            expression(p);
            p.expect(RBracket);
        }
        if p.at(Dot) {
            p.expect(Dot);
            p.expect(Identifier);
            p.close(m_id, ComponentCall);
        } else {
            p.close(m_id, Expression);
        }
    } else {
        expression(p);
    }

    if p.at_any(&[
        Assign,
        RAssignSignal,
        RAssignConstraintSignal,
        LAssignContraintSignal,
        LAssignSignal,
        EqualSignal,
    ]) {
        p.advance();
        expression(p);
        p.close(m, AssignStatement);
    } else {
        p.close(m, Error);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn if_statement_test() {
        let _source = r#"
            assert(1 == 2);
        "#;
        // let mut parser = Parser::new(source);

        // statement(&mut parser);
        // let cst = parser.build_tree().ok().unwrap();

        // println!("{:?}", cst);
    }
}
