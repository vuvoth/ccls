use super::*;

/// Parse a block `{ <declaration>|<statement> ... }` (grammar: `ParseBlock`).
pub fn block(p: &mut Parser) {
    if !p.at(LCurly) {
        p.advance_with_error("expected `{`");
    } else {
        let m = p.open();
        p.expect(LCurly);

        let stmt_marker = p.open();
        while !p.at(RCurly) && !p.eof() {
            let kind = p.current();
            match kind {
                SignalKw | InputKw | OutputKw => {
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

        p.expect(RCurly);
        p.close(m, Block);
    }
}
