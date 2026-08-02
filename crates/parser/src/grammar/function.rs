use definition::definition_body;

use crate::grammar::*;

// fucntion name()
pub fn function_parse(p: &mut Parser) {
    let m = p.open();

    p.expect(FunctionKw);

    definition_body(p, FunctionName);

    p.close(m, FunctionDef);
}
