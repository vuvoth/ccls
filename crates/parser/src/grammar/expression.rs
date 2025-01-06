use list::tuple_expression;

use crate::parser::Marker;

use super::*;

pub(super) fn expression(p: &mut Parser) {
    let open_marker = p.open();
    circom_expression(p);
    p.close(open_marker, Expression);
}

/**
 * TODO: why parse a stament inside expression module???
 * manage 2 cases: normal expression (a++, a-b,...), tenary_conditional_statement (a ? b : c)
 * circom_expression = expr ? expr: expr |
 *                     expr                          
 */
fn circom_expression(p: &mut Parser) {
    if let Some(lhs) = expression_rec(p, 0) {
        let current_kind = p.current();

        if matches!(current_kind, MarkQuestion) {
            tenary_conditional_statement(p, lhs);
        }
    }
}

/**
 * grammar: <condition> ? <expression-1> : <expression-2>
* <condition> is also an expression,
* whose open and close events are already in the Parser event list
* lhs is that open event
*/
pub fn tenary_conditional_statement(p: &mut Parser, lhs: Marker) {
    // <condition>
    let open_marker = p.open_before(lhs);
    p.close(open_marker, Condition);

    // <condition> ?
    p.expect(MarkQuestion);

    // <condition> ? <expression-1>
    let first_expression = p.open();
    expression_rec(p, 0);
    p.close(first_expression, Expression);

    // <condition> ? <expression-1> :
    p.expect(Colon);

    // <condition> ? <expression-1> : <expression-2>
    let last_expression = p.open();
    expression_rec(p, 0);
    p.close(last_expression, Expression);

    p.close(open_marker, TenaryConditional);
}

/**
 * return marker which bound the expression
 */
pub fn expression_rec(p: &mut Parser, pb: u16) -> Option<Marker> {
    // consume all first prefix tokens (++a, --a, -a, +a, !a)
    // next, consume first atom (identifier/number/tuple)
    let parse_able: Option<Marker> = {
        if let Some(pp) = p.current().prefix() {
            let kind = p.current();
            let open_marker = p.open();
            // consume prefix token (++, --, -, +, !)
            p.advance();
            // continue with the next tokens
            expression_rec(p, pp);
            Some(p.close(open_marker, kind))
        } else {
            expression_atom(p)
        }
    };

    parse_able?;

    let mut lhs = parse_able.unwrap();

    while !p.eof() {
        let kind = p.current();

        if let Some((lp, rp)) = kind.infix() {
            // infix case: <a> + <b>
            // <a> is already consume in parse_able

            // TODO: what does it mean???
            if rp <= pb {
                return None;
            }

            // open event that wrap the first parameter (<a>)
            let open_marker = p.open_before(lhs);

            // consume the infix token
            p.advance();

            // extract the second parameter
            expression_rec(p, lp);

            lhs = p.close(open_marker, kind);
        } else if let Some(pp) = kind.postfix() {
            if pp <= pb {
                return None;
            }

            match kind {
                LParen => {
                    // function call
                    let open_marker = p.open_before(lhs);
                    tuple_expression(p);
                    lhs = p.close(open_marker, Call);
                }
                LBracket => {
                    // array subscript: abc[N - 1]
                    let open_marker = p.open_before(lhs);
                    p.expect(LBracket);
                    expression(p);
                    p.expect(RBracket);
                    p.close(open_marker, ArrayQuery);
                }
                Dot => {
                    // attribute access
                    // abc[N - 1].def OR abc.def --> component call
                    let open_marker = p.open_before(lhs);
                    p.expect(Dot);
                    p.expect(Identifier);
                    p.close(open_marker, ComponentCall);
                }
                UnitDec | UnitInc => {
                    let open_marker = p.open_before(lhs);
                    // consume token ++/-- and do nothing
                    p.advance();
                    p.close(open_marker, Expression);
                }
                _ => {
                    // not a postfix token
                    p.advance_with_error(&format!("Expect a postfix token, but found {:?}", kind));
                    break;
                }
            };
        } else {
            break;
        }
    }

    // return the outer open marker
    Some(lhs)
}

/**
 * the unit element in expression
 * eg: a, b, 5, 100, (<expression>)
 */
fn expression_atom(p: &mut Parser) -> Option<Marker> {
    let kind = p.current();

    match kind {
        Number | Identifier => {
            let open_marker = p.open();
            p.advance();
            let m_close = p.close(open_marker, kind);
            Some(m_close)
        }
        LParen => {
            // (<expression>)
            let open_marker = p.open();
            p.expect(LParen);
            expression_rec(p, 0);
            p.expect(RParen);
            let m_close = p.close(open_marker, Expression);
            Some(m_close)
        }
        _ => {
            p.advance_with_error("Invalid Token");
            None
        }
    }
}
