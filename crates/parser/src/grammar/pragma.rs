use super::*;

/// Parse a pragma directive (grammar: `ParsePragma`).
///
/// Two forms:
/// - `pragma circom <Version>;` — version pin.
/// - `pragma custom_templates;` — enable custom gates.
///
/// Anything else after `pragma` is recovered with an error.
pub fn pragma(p: &mut Parser) {
    let m = p.open();
    p.expect(PragmaKw);
    if p.at(Circom) {
        p.advance();
        p.expect(Version);
    } else if p.at(CustomTemplatesKw) {
        p.advance();
    } else {
        // expected `circom` or `custom_templates` after `pragma`
        p.advance_with_error();
    }
    p.expect(Semicolon);
    p.close(m, Pragma);
}
