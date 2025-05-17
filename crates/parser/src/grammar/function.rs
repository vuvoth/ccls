use block::block;
use list::tuple_identifier;

use crate::grammar::*;

/// Functions define generic abstract pieces of code that can perform some computations
/// to obtain a value or an expression to be returned.
///
/// Grammar:
/// * `function <FunctionName> <ParameterList> <FunctionBody>`
///
///  Example:
///  ```ignore
///  function nbits(a) {
///     var n = 1;
///     var r = 0;
///     while (n-1<a) {
///         r++;
///         n *= 2;
///     }
///     return r;
///  }
/// ```
/// * `<FunctionName>`: `nbits`
/// * `<ParameterList>`: `(a)`
/// * `<FunctionBody>`: `{ var n = 1; ... return r; }`
///
pub fn function(p: &mut Parser) {
    let m = p.open();

    p.expect(FunctionKw);

    let name_marker = p.open();
    p.expect(Identifier);
    p.close(name_marker, FunctionName);

    let parameter_marker = p.open();
    tuple_identifier(p);
    p.close(parameter_marker, ParameterList);

    let body_marker = p.open();
    block(p);
    p.close(body_marker, FunctionBody);

    p.close(m, FunctionDef);
}
