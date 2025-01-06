use list::tuple_identifier;

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

    let parameter_marker = p.open();
    tuple_identifier(p);
    p.close(parameter_marker, ParameterList);

    block::block(p);

    p.close(m, TemplateDef);
}
