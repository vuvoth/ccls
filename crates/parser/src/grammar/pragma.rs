use super::*;

/**
 * parse pragma in circom language
 * grammar: 
 *      pragma circom <version>;
 */


pub fn pragma(p: &mut Parser) {
    p.expect(Pragma);
    p.expect(Circom);
    p.expect(Version);
}