use super::*;

// a, b, c, d
pub fn parse(p: &mut Parser) {
    while p.at(Identifier) && !p.eof() {
        p.expect(Identifier);
        
        if p.eat(Comma) == false {
            break;
        }
    }
}
