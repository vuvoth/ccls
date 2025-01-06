use crate::parser::Marker;

use super::*;

pub(super) fn expression(p: &mut Parser) {
    let m = p.open();
    circom_expression(p);
    p.close(m, Expression);
}

/**
 * grammar: "(param1, param2,..., paramn)"
 * can be an empty ()
 */
pub(super) fn function_params(p: &mut Parser) {
    let m = p.open();
    p.expect(LParen);

    while !p.at(RParen) && !p.eof() {
        expression(p);
        if p.at(Comma) {
            p.expect(Comma)
        } else {
            break;
        }
    }

    p.expect(RParen);
    // TODO: what kind of it?
    p.close(m, Tuple);
}

/**
 * grammar: "(Symbol_1, Symbol_2,..., Symbol_n)"
 */
pub(super) fn tuple(p: &mut Parser) {
    let m = p.open();
    p.expect(LParen);

    // iden1, iden2, iden3
    list_identity::parse(p);

    p.expect(RParen);
    p.close(m, Tuple);
}

/**
 * grammar:
 *      "= | <== | <--" expression
 */
pub(super) fn tuple_init(p: &mut Parser) {
    let m = p.open();
    p.expect_any(&[Assign, RAssignSignal, RAssignConstraintSignal]);
    expression(p);
    p.close(m, TupleInit);
}

fn expression_atom(p: &mut Parser) -> Option<Marker> {
    let m_close: Marker;
    match p.current() {
        Number => {
            let m = p.open();
            p.advance();
            m_close = p.close(m, Number);
            Some(m_close)
        }
        Identifier => {
            let m = p.open();
            p.advance();
            m_close = p.close(m, Identifier);
            Some(m_close)
            // identifier(p)
        }
        LParen => {
            let m = p.open();
            p.expect(LParen);
            expression_rec(p, 0);
            p.expect(RParen);
            m_close = p.close(m, Tuple);
            Some(m_close)
        }
        _ => {
            p.advance_with_error("Invalid Token");
            None
        }
    }
}

/**
 * return marker which bound the expression
 */
pub fn expression_rec(p: &mut Parser, pb: u16) -> Option<Marker> {
    // consume all first prefix tokens (++a, --a, -a, +a, !a)
    // next, consume first atom (identifier/number/tuple)
    let parse_able: Option<Marker> = {
        if let Some(pp) = p.current().prefix() {
            println!("Prefix...");
            let kind = p.current();
            let m = p.open();
            // consume prefix token (++, --, -, +, !)
            p.advance();
            // continue with the next tokens
            expression_rec(p, pp);
            Some(p.close(m, kind))
        } else {
            expression_atom(p)
        }
    };

    parse_able?;

    let mut lhs = parse_able.unwrap();

    while !p.eof() {
        let current_kind = p.current();

        if let Some((lp, rp)) = current_kind.infix() {
            // TODO: what does it mean???
            if rp <= pb {
                return None;
            }

            let m = p.open_before(lhs);
            // consume the infix token
            p.advance();

            // extract the second parameter
            // eg: <a> + <b> --> extract <b>
            expression_rec(p, lp);
            lhs = p.close(m, current_kind);

        } else if let Some(pp) = current_kind.postfix() {
            println!("Postfix...");
            if pp <= pb {
                return None;
            }

            match current_kind {
                LParen => {
                    // function call
                    let m = p.open_before(lhs);
                    function_params(p);
                    lhs = p.close(m, Call);
                }
                LBracket => {
                    // array subscript: abc[N - 1]
                    let m = p.open_before(lhs);
                    p.expect(LBracket);
                    expression(p);
                    p.expect(RBracket);
                    p.close(m, ArrayQuery);
                }
                Dot => {
                    // attribute access
                    // abc[N - 1].def OR abc.def --> component call
                    let m = p.open_before(lhs);
                    p.expect(Dot);
                    p.expect(Identifier);
                    p.close(m, ComponentCall);
                }
                UnitDec | UnitInc => {
                    let m = p.open_before(lhs);
                    // consume token and do nothing
                    p.advance();
                    p.close(m, Expression);
                }
                _ => {
                    // not a postfix token
                    p.advance_with_error(&format!("Expect a postfix token, but found {:?}", current_kind));
                }
            };
        }
        else {
            break;
        }
    }
    
    Some(lhs)
}

/**
 * circom_expression = expr ? expr: expr |
 *                     expr                          
 */
fn circom_expression(p: &mut Parser) {
    if let Some(mut lhs) = expression_rec(p, 0) {
        let current_kind = p.current();
        if matches!(current_kind, MarkQuestion) {
            let m = p.open_before(lhs);
            lhs = p.close(m, Condition);

            let m = p.open_before(lhs);
            p.advance();

            let first_expression = p.open();
            expression_rec(p, 0);
            p.close(first_expression, Expression);

            p.expect(Colon);

            let last_expression = p.open();
            expression_rec(p, 0);
            p.close(last_expression, Expression);

            p.close(m, TenaryConditional);
        }
    }
}
