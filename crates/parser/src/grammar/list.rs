use crate::grammar::{expression::expression, *};

/// Argument list used to pass parameters to a template/function instantiation.
///
/// Grammar: `(expression-1, expression-2,..., expression-n)`
///
/// Example:
/// ```ignore
/// component comp = MyTemplate( 2,3 );
/// ```
/// * `expression-1`: `2`
/// * `expression-2`: `3`
pub fn argument_list(p: &mut Parser) {
    let m = p.open();

    tuple_expression(p);

    p.close(m, ArgumentList);
}

/// Parameter list used to pass parameters to a template/function definition.
///
/// Grammar: `(iden-1, iden-2,..., iden-n)`
/// Can be an empty `()`.
///
/// Example:
/// ```ignore
/// template MyTemplate( a, b ) {
/// ...
/// }
/// ```
/// * `iden-1`: `a`
/// * `iden-2`: `b`
pub fn parameter_list(p: &mut Parser) {
    let m = p.open();

    tuple_identifier(p);

    p.close(m, ParameterList);
}

/// Grammar: `(expression-1, expression-2,..., expression-n)`
/// Can be an empty `()`.
pub(super) fn tuple_expression(p: &mut Parser) {
    p.expect(LParen);

    while !p.at(RParen) && !p.eof() {
        expression(p);

        if !p.eat(Comma) {
            break;
        }
    }

    p.expect(RParen);
}

/// Grammar: `(iden-1, iden-2,..., iden-n)`
/// Can be an empty `()`.
pub(super) fn tuple_identifier(p: &mut Parser) {
    p.expect(LParen);

    while p.at(Identifier) && !p.eof() {
        p.expect(Identifier);

        if !p.eat(Comma) {
            break;
        }
    }

    p.expect(RParen);
}
