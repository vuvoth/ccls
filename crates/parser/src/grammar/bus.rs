use definition::definition_body;

use crate::grammar::*;

/// Parse a bus definition (grammar: `ParseDefinition` → `bus <IDENT> <ParseParenthesisArguments?> <ParseBlock>`).
///
/// Mirrors template/function: `bus Name` then the shared optional-params + block tail. Gaps 13/14.
pub fn bus_definition(p: &mut Parser) {
    let m = p.open();
    p.expect(BusKw);
    definition_body(p, BusName);
    p.close(m, BusDef);
}
