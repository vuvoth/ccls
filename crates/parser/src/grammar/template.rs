use definition::definition_body;

use crate::grammar::*;

/**
 * template Identifier() {content}
 * template Identifier( param_1, ... , param_n ) { content }
 *
 * Optional modifiers precede the name in the fixed grammar order
 * `template <"custom"?> <"extern_c"?> <"parallel"?> <IDENT> …`.
 */
pub fn template(p: &mut Parser) {
    // assert!(p.at(TemplateKw));
    let m = p.open();

    p.expect(TemplateKw);

    // Fixed-order optional modifiers (grammar ParseDefinition).
    p.eat(CustomKw);
    p.eat(ExternCKw);
    p.eat(ParallelKw);

    definition_body(p, TemplateName);

    p.close(m, TemplateDef);
}
