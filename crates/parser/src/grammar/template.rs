use crate::grammar::*;
/**
 * template Identifier() {content}
 * template Identifier( param_1, ... , param_n ) { content }
 */
pub fn template(p: &mut Parser) {
    // assert!(p.at(TemplateKw));
    let m = p.open();

    p.expect(TemplateKw);

    let name_marker = p.open();
    p.expect(Identifier);
    p.close(name_marker, TemplateName);

    p.expect(LParen);
    let arg_marker = p.open();
    list_identity::parse(p);
    p.close(arg_marker, ParameterList);
    p.expect(RParen);

    block::block(p);

    p.close(m, TemplateDef);
}
