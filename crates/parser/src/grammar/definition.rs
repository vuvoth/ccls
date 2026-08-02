use block::block;
use list::tuple_identifier;

use crate::grammar::*;
use crate::token_kind::TokenKind;

/// `<name> (<identifier-params>)? <block>` — the shared tail of `template`/`function`/`bus`
/// definitions (grammar: an `IDENT` followed by an *optional* parenthesized parameter list and a
/// `ParseBlock`).
///
/// `name_kind` is the node kind wrapping the name token (`TemplateName` / `FunctionName` /
/// `BusName`); the params — when present — are wrapped in a `ParameterList`, and the body is the
/// existing `block::block`.
///
/// The parameter list is optional per the grammar (`ParseParenthesisArguments?`): `template T {}`
/// (no parens) is valid. Guarding with `at(LParen)` is a no-op for every existing test file (which
/// all use `()`), so this extraction is snapshot-neutral.
pub(super) fn definition_body(p: &mut Parser, name_kind: TokenKind) {
    let name_marker = p.open();
    p.expect(Identifier);
    p.close(name_marker, name_kind);

    let parameter_marker = p.open();
    if p.at(LParen) {
        tuple_identifier(p);
    }
    p.close(parameter_marker, ParameterList);

    block(p);
}
