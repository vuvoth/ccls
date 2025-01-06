use list::tuple_identifier;

use crate::grammar::*;

// fucntion name()
pub fn function_parse(p: &mut Parser) {
    let m = p.open();

    p.expect(FunctionKw);

    let fn_name_marker = p.open();
    p.expect(Identifier);
    p.close(fn_name_marker, FunctionName);

    let parameter_marker = p.open();
    tuple_identifier(p);
    p.close(parameter_marker, ParameterList);

    block::block(p);

    p.close(m, FunctionDef);
}
