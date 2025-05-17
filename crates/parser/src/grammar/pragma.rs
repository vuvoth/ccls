use super::*;

/**
 * parse pragma in circom language
 * grammar:
 *      pragma circom <version>;
 * eg:
 *     pragma circom 2.0.0;
 */
pub fn pragma(p: &mut Parser) {
    let m = p.open();
    p.expect(PragmaKw);
    p.expect(CircomKw);
    p.expect(Version);
    p.expect(Semicolon);
    p.close(m, Pragma);
}
