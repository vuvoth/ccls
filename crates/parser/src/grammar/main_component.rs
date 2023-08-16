use super::*;


pub fn main_component(p: &mut Parser) {
    p.expect(ComponentKw);
    p.expect(MainKw);
    p.expect(LCurly);
    p.expect(PublicKw);
    p.expect(LBracket);
    list_identity::parse(p);
    p.expect(RBracket);
    p.expect(Assign);
    expression::expression(p);
}