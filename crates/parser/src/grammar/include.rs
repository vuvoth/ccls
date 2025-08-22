use super::*;

/// In order to use code in other files, we have to
/// include them in our program.
///
/// Grammar:
/// * `include "<filename>";`
///
/// Example:
/// ```ignore
/// include "circuit.circom";
/// ```
pub(super) fn include(p: &mut Parser) {
    let m = p.open();
    p.expect(IncludeKw);
    p.expect(CircomString);
    p.expect(Semicolon);
    p.close(m, Include);
}
