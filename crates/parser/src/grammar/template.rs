
use crate::grammar::*;
use super::*;
/**
 * template Identifier() {content}
 *
 */
pub fn template(p: &mut Parser) {
    assert!(p.at(TemplateKw));
    let m = p.open();
    p.expect(TemplateKw);
    p.expect(Identifier);
    p.expect(LParen);
    p.expect(RParen);
    block::block(p);
    p.close(m, TemplateKw);
}

pub fn function_parse(p: &mut Parser) {
    let m = p.open();
    p.expect(FunctionKw);
    p.expect(Identifier);
    p.expect(LParen);
    p.expect(RParen);
    block::block(p);
    p.close(m, FunctionDef);
}