use crate::grammar::*;
use block::block;
use list::parameter_list;

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

    function_name(p);

    parameter_list(p);

    let body_marker = p.open();
    block(p);
    p.close(body_marker, FunctionBody);

    p.close(m, FunctionDef);
}

/// Function name used to identify a function.
///
/// Grammar: `Identifier`
///
/// Example:
/// ```ignore
/// MyFunction
/// ```
/// * `Identifier`: `MyFunction`.
pub fn function_name(p: &mut Parser) {
    let m = p.open();
    p.expect(Identifier);
    p.close(m, TemplateName);
}
