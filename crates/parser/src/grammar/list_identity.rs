use super::*;


pub fn parse(p: &mut Parser) {
    while p.at(Identifier) && !p.eof() {
        p.expect(Identifier);
        if p.at(Comma) {
            p.expect(Comma)
        } else {
            break;
        }
    }
}