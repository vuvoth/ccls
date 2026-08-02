use crate::grammar::{expression::expression, *};

/// One or more comma-separated expressions (grammar: a `Listable`/`TwoElemsListable` tail). Used by
/// call argument lists, inline arrays, and tuple expressions (all share the same element syntax).
pub(super) fn expression_list(p: &mut Parser) {
    expression(p);
    while p.eat(Comma) {
        expression(p);
    }
}

/**
 * grammar: "(expression-1, expression-2,..., expression-n)"
 * can be an empty () — the parenthesized argument/expression list of a call. (This is NOT a tuple
 * expression; a real `TupleExpr` node exists for the ≥2-element literal form.)
 */
pub(super) fn paren_list(p: &mut Parser) {
    p.expect(LParen);

    // expression-1, expression-2,..., expression-n)
    while !p.at(RParen) && !p.eof() {
        expression(p);

        // there are no expressions remaining
        if !p.eat(Comma) {
            break;
        }
    }

    p.expect(RParen);
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

        if !p.eat(Comma) {
            break;
        }
    }

    p.expect(RParen);
    // p.close(m, IdentifierList);
}

/**
 * grammar: "[iden1, iden2,..., idenn]"
 * can be an empty ()
 * only use in main component.
 */
pub(super) fn list_identifier(p: &mut Parser) {
    // let m = p.open();
    p.expect(LBracket);

    // iden1, iden2, iden3
    while p.at(Identifier) && !p.eof() {
        p.expect(Identifier);

        if !p.eat(Comma) {
            break;
        }
    }

    p.expect(RBracket);
    // p.close(m, IdentifierList);
}
