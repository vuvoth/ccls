use super::*;

pub(super) fn include(p: &mut Parser) {
    assert!(p.at(IncludeKw));

    let m = p.open();
    p.expect(IncludeKw);
    p.expect(CircomString);
    p.expect(Semicolon);
    p.close(m, IncludeKw);
}
