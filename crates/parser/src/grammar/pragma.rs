use super::*;

/// Parse `pragma circom <version>;` (grammar: `ParsePragma`).
pub fn pragma(p: &mut Parser) {
    let m = p.open();
    p.expect(PragmaKw);
    p.expect(Circom);
    p.expect(Version);
    p.expect(Semicolon);
    p.close(m, Pragma);
}
