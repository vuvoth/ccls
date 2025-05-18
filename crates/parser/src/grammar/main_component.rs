use list::list_identifier;

use super::*;

/*
component main {public [signal_list]} = tempid(v1,...,vn);

{public [signal_list]} is optional
*/
pub fn main_component(p: &mut Parser) {
    let open_marker = p.open();

    // component main
    p.expect(ComponentKw);
    p.expect(MainKw);

    // {public [signal_list]}
    if p.at(LCurly) {
        p.expect(LCurly);
        p.expect(PublicKw);
        list_identifier(p);
        p.expect(RCurly);
    }

    // = tempid(v1,...,vn);
    p.expect(Assign);
    expression::expression(p);
    p.expect(Semicolon);

    p.close(open_marker, MainComponent);
}
