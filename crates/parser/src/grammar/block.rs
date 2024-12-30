use super::*;

/*
{
    <declaration>/<statement>
    <declaration>/<statement>
    ....
    <declaration>/<statement>
}
*/
pub fn block(p: &mut Parser) {
    p.inc_rcurly();

    // TODO: why do not use expect for { and }
    if !p.at(LCurly) {
        p.advance_with_error("Miss {");
    } else {
        let m = p.open();
        p.eat(LCurly);
        let stmt_marker = p.open();
        while !p.at(RCurly) && !p.eof() {
            let kind = p.current();
            match kind {
                SignalKw => {
                    declaration::signal_declaration(p);
                    p.expect(Semicolon);
                }
                VarKw => {
                    declaration::var_declaration(p);
                    p.expect(Semicolon);
                }
                ComponentKw => {
                    declaration::component_declaration(p);
                    p.expect(Semicolon);
                }
                _ => statement::statement(p),
            }
        }

        p.close(stmt_marker, StatementList);

        p.eat(RCurly);

        p.close(m, Block);

        p.dec_rcurly();
    }
}
