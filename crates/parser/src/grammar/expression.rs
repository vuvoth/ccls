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
 * can be an empty tuple (for function cal: Mul())
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
    let parse_able: Option<Marker> = if let Some(pp) = p.current().prefix() {
        let kind = p.current();
        let m = p.open();
        p.advance();
        expression_rec(p, pp);
        Some(p.close(m, kind))
    } else {
        expression_atom(p)
    };

    parse_able?;

    let mut lhs = parse_able.unwrap();

    // TODO: function call
    if p.at(LParen) {
        let m = p.open_before(lhs);
        // tuple(p);
        function_params(p);
        lhs = p.close(m, Call);
    }

    while !p.eof() {
        let current_kind = p.current();
        if let Some((lp, rp)) = current_kind.infix() {
            if rp <= pb {
                return None;
            }

            let m = p.open_before(lhs);
            p.advance();
            expression_rec(p, lp);
            lhs = p.close(m, current_kind);

            continue;
        }
        if let Some(pp) = current_kind.postfix() {
            if pp <= pb {
                return None;
            }
            let m = p.open_before(lhs);
            p.advance();
            if matches!(current_kind, LBracket) {
                expression_rec(p, 0);
                p.expect(RBracket);
            } else {
                p.expect(Identifier);
            }
            lhs = if matches!(current_kind, Dot) {
                p.close(m, ComponentCall)
            } else {
                p.close(m, ArrayQuery)
            };

            continue;
        }
        break;
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
