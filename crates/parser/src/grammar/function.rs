use crate::grammar::*;

pub fn function_parse(p: &mut Parser) {
    let m = p.open();
    p.expect(FunctionKw);
    let fn_name_marker = p.open();

    p.expect(Identifier);
    p.close(fn_name_marker, FunctionName);
    p.expect(LParen);
    let arg_marker = p.open();
    while !p.at(RParen) && !p.eof() {
        p.expect(Identifier);
        if p.at(Comma) {
            p.expect(Comma);
        }
    }

    p.close(arg_marker, ParameterList);

    p.expect(RParen);

    block::block(p);
    p.close(m, FunctionDef);
}
