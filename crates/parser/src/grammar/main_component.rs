use list::list_identifier;

use super::*;

/*
component main {public [signal_list]} = tempid(v1,...,vn);

{public [signal_list]} is optional
*/
pub fn main_component(p: &mut Parser) {
    p.expect(ComponentKw);
    p.expect(MainKw);

    if p.at(LCurly) {
        p.expect(LCurly);
        p.expect(PublicKw);
        p.expect(LBracket);
        list_identifier(p);
        p.expect(RBracket);
    }

    p.expect(Assign);
    expression::expression(p);
}
