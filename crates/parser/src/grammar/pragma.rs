use super::*;

/// `pragma` ensures that the circuit is compatible with the
/// specified compiler version.
///
/// Grammar:
/// * `pragma circom <version>;`
///
/// Example:
/// ```ignore
/// pragma circom 2.0.0;
/// ```
pub fn pragma(p: &mut Parser) {
    let m = p.open();
    p.expect(PragmaKw);
    p.expect(CircomKw);
    p.expect(Version);
    p.expect(Semicolon);
    p.close(m, Pragma);
}
