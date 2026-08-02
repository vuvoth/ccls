use super::*;

/// Parse a block `{ <declaration>|<statement> ... }` (grammar: `ParseBlock`).
pub fn block(p: &mut Parser) {
    if !p.at(LCurly) {
        // expected `{`
        p.advance_with_error();
    } else {
        let m = p.open();
        p.expect(LCurly);

        let stmt_marker = p.open();
        while !p.at(RCurly) && !p.eof() {
            let kind = p.current();
            match kind {
                SignalKw => {
                    declaration::signal_declaration(p);
                    p.expect(Semicolon);
                }
                // `input`/`output` is either a signal declaration (`input signal …`) or a bus-typed
                // field (`input <Bus> …`). The dispatch (and its `nth(1)` lookahead) lives in one
                // place — `declaration::input_or_output` — shared with the `for`-init path.
                InputKw | OutputKw => {
                    declaration::input_or_output(p);
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
