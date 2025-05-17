use crate::grammar::*;
use block::block;
use list::{argument_list, tuple_identifier};

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

    template_name(p);

    let parameter_marker = p.open();
    tuple_identifier(p);
    p.close(parameter_marker, ParameterList);

    let body_marker = p.open();
    block(p);
    p.close(body_marker, TemplateBody);

    p.close(m, TemplateDef);
}

/// Template name used to identify a template.
///
/// Grammar: `Identifier`
///
/// Example:
/// ```ignore
/// MyTemplate
/// ```
/// * `Identifier`: `MyTemplate`.
pub fn template_name(p: &mut Parser) {
    let m = p.open();
    p.expect(Identifier);
    p.close(m, TemplateName);
}

/// Template instantiation used to apply a template with concrete parameters.
///
/// Grammar: `TemplateName(Arg1, Arg2, ..., ArgN)`
///
/// Example:
/// ```ignore
/// MyTemplate(8, true)
/// ```
/// * `TemplateName`: `MyTemplate`.
/// * `(Arg1...ArgN)`: `(8, true)`.
///
/// Notes:
/// - Argument types must match the template's parameter specification.
/// - Used wherever generic or reusable logic is needed.
pub fn template_instantiation(p: &mut Parser) {
    let m = p.open();

    template_name(p);

    argument_list(p);

    p.close(m, TemplateInstantiation);
}
