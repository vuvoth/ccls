use super::{block::block, expression::expression, *};

pub(super) fn statement(p: &mut Parser) {
    let open_marker = p.open();
    match p.current() {
        IfKw => if_statement(p),
        _ => statement_no_condition(p),
    }
    p.close(open_marker, Statement);
}

/*
if (expr)
    <statement>
else
    <statement>
*/
fn if_statement(p: &mut Parser) {
    let open_marker = p.open();

    // if (<condition>) <statement>
    p.expect(IfKw);
    p.expect(LParen);
    expression(p);
    p.expect(RParen);
    statement(p);

    // else <statement>
    if p.at(ElseKw) {
        p.expect(ElseKw);
        statement(p);
    }

    p.close(open_marker, IfStatement);
}

/**
 * no if condition here.
 * for/while/return/assert...
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

/*
for (<declaration>/<assignment>; <expression>; <assignment>)
    <statement>
*/
fn for_statement(p: &mut Parser) {
    let open_marker = p.open();

    // for (
    p.expect(ForKw);
    p.expect(LParen);

    if p.current().is_declaration_kw() {
        // for (var i = 1
        declaration::declaration(p);
    } else {
        // for (i = 1
        assignment_statement(p);
    }
    p.expect(Semicolon);

    // for (i = 1; i < N;
    expression::expression(p);
    p.expect(Semicolon);

    // for (i = 1; i < N; i++)
    assignment_statement(p);
    p.expect(RParen);

    // for (i = 1; i < N; i++) { <statements> }
    statement(p);

    p.close(open_marker, ForLoop);
}

/*
while (<expression>)
    <statement>
*/
fn while_statement(p: &mut Parser) {
    let open_marker = p.open();

    p.expect(WhileKw);
    p.expect(LParen);
    expression(p);
    p.expect(RParen);
    statement(p);

    p.close(open_marker, WhileLoop);
}

/*
assert(<expression>)
*/
fn assert_statement(p: &mut Parser) {
    let open_marker = p.open();

    p.expect(AssertKw);
    p.expect(LParen);
    expression(p);
    p.expect(RParen);

    p.close(open_marker, AssertStatement);
}

/*
log(<pattern1>, <pattern2>, ... <patternn>)
*/
fn log_statement(p: &mut Parser) {
    let open_marker = p.open();

    p.expect(LogKw);
    p.expect(LParen);

    // list circom string/expression
    while !p.eof() {
        match p.current() {
            RParen => break,
            CircomString => p.advance(),
            _ => expression(p),
        }

        if p.eat(Comma) == false {
            break;
        }
    }

    p.expect(RParen);

    p.close(open_marker, LogStatement);
}

/*
return <expression>
*/
fn return_statement(p: &mut Parser) {
    let open_marker = p.open();
    p.expect(ReturnKw);
    expression(p);
    p.close(open_marker, ReturnStatement);
}

/*
<left-expression> <assignment-token> <right-expression>
optional: <assignment-token> <right-expression>
eg: out[1] <== in[0] + in[2]
*/
fn assignment_statement(p: &mut Parser) {
    let open_marker = p.open();

    // left expression
    expression(p);

    // assign part
    if p.at_assign_token() {
        p.advance();

        // right expression
        expression(p);
    }

    p.close(open_marker, AssignStatement);
}
