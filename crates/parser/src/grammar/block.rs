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
            if p.current().is_declaration_kw() {
                // Single dispatch source: `declaration::declaration` (and its `input_or_output`
                // bus-typed lookahead) is shared with the `for`-init path, so the declaration
                // keyword set lives in exactly one place — `is_declaration_kw`.
                declaration::declaration(p);
                p.expect(Semicolon);
            } else {
                statement::statement(p);
            }
        }

        p.close(stmt_marker, StatementList);

        p.expect(RCurly);
        p.close(m, Block);
    }
}
