use crate::grammar::{expression::expression, *};

/**
 * grammar: "(expression-1, expression-2,..., expression-n)"
 * can be an empty ()
 */
pub(super) fn tuple_expression(p: &mut Parser) {
    // let m = p.open();
    p.expect(LParen);

    // expression-1, expression-2,..., expression-n)
    while !p.at(RParen) && !p.eof() {
        expression(p);

        // there are no expressions remaining
        if p.eat(Comma) == false {
            break;
        }
    }

    p.expect(RParen);

    // p.close(m, ExpressionList);
}

/**
 * grammar: "(iden1, iden2,..., idenn)"
 * can be an empty ()
 */
pub(super) fn tuple_identifier(p: &mut Parser) {
    // let m = p.open();
    p.expect(LParen);

    // iden1, iden2, iden3
    while p.at(Identifier) && !p.eof() {
        p.expect(Identifier);

        if p.eat(Comma) == false {
            break;
        }
    }

    p.expect(RParen);
    // p.close(m, IdentifierList);
}

/**
 * grammar: "[iden1, iden2,..., idenn]"
 * can be an empty ()
 */
pub(super) fn list_identifier(p: &mut Parser) {
    // let m = p.open();
    p.expect(LBracket);

    // iden1, iden2, iden3
    while p.at(Identifier) && !p.eof() {
        p.expect(Identifier);

        if p.eat(Comma) == false {
            break;
        }
    }

    p.expect(RBracket);
    // p.close(m, IdentifierList);
}
