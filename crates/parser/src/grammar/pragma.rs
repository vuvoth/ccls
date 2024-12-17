use super::*;

/**
 * parse pragma in circom language
 * grammar:
 *      pragma circom <version>;
 */

pub fn pragma(p: &mut Parser) {
    let m = p.open();
    p.expect(Pragma);
    p.expect(Circom);
    p.expect(Version);
    p.expect(Semicolon);
    p.close(m, Pragma);
}
