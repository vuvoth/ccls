use block::block;
use list::tuple_identifier;

use crate::grammar::*;

/// Templates in circom are circuit blueprints with parameters.
/// When used, they create circuits with their own signals,
/// usable inside larger circuits.
///
/// Grammar:
/// * `template <TemplateName> <ParameterList> <TemplateBody>`
///
///  Example:
///  ```ignore
///  template MyTemplate( a, b ) {
///     signal input a;
///     signal input b;
///     signal output c;
///     c <== a + b;
///  }
///  ```
/// * `<TemplateName>`: `MyTemplate`
/// * `<ParameterList>`: `( a, b )`
/// * `<TemplateBody>`: `{ signal input a; signal input b; signal output c; c <== a + b; }`
///
pub fn template(p: &mut Parser) {
    let m = p.open();

    p.expect(TemplateKw);

    let name_marker = p.open();
    p.expect(Identifier);
    p.close(name_marker, TemplateName);

    let parameter_marker = p.open();
    tuple_identifier(p);
    p.close(parameter_marker, ParameterList);

    let body_marker = p.open();
    block(p);
    p.close(body_marker, TemplateBody);

    p.close(m, TemplateDef);
}

/// TemplateName(2, 15)
pub fn template_instantiation(p: &mut Parser) {
    let m = p.open();

    let name_marker = p.open();
    p.expect(Identifier);
    p.close(name_marker, TemplateName);

    let args_marker = p.open();
    if p.at(LParen) {
        tuple_identifier(p);
    }
    p.close(args_marker, ArgumentList);

    p.close(m, TemplateInstantiation);
}
