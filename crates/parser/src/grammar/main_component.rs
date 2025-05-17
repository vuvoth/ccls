use super::*;
use template::template_instantiation;

/// In Circom, execution starts from a specific component—by default named `main`.
/// This component must be instantiated using a template, optionally exposing
/// certain signals as public.
///
/// Grammar:
/// * `component <ComponentName> [ {public [<SignalList>]} ] = <TemplateName>(<ArgumentList>);`
///
/// Example:
/// ```ignore
/// pragma circom 2.0.0;
///
/// template A() {
///     signal input in1;
///     signal input in2;
///     signal output out;
///     out <== in1 * in2;
/// }
///
/// component main {public [in1]} = A();
/// ```
/// * `<ComponentName>`: `main`
/// * `<SignalList>`: `[in1]` (signals exposed as public inputs)
/// * `<TemplateId>`: `A` (template used to instantiate the component)
/// * `<Args>`: `()` (arguments passed to the template)
pub fn main_component(p: &mut Parser) {
    let open_marker = p.open();

    p.expect(ComponentKw);
    p.expect(MainKw);

    if p.at(LCurly) {
        public_signals(p);
    }

    p.expect(Assign);

    template_instantiation(p);

    p.expect(Semicolon);

    p.close(open_marker, MainComponent);
}

/// Public signals part is an optional part in main component
///
/// Grammar: `{public [<SignalList>]}`
///
/// Example:
/// ```ignore
/// {public [in1, in2]}
/// ```
/// * `<SignalList>`: `[in1, in2]` (signals exposed as public inputs)
pub fn public_signals(p: &mut Parser) {
    let open_marker = p.open();

    p.expect(LCurly);
    p.expect(PublicKw);
    list_identifier(p);
    p.expect(RCurly);

    p.close(open_marker, PublicSignals);
}

/// Identifier list used only in the main component. It cannot be empty.
///
/// Grammar: `[<Identifier1>, <Identifier2>, ..., <IdentifierN>]`
///
/// Example:
/// ```ignore
/// [iden1, iden2]
/// ```
pub(super) fn list_identifier(p: &mut Parser) {
    p.expect(LBracket);

    // at least one identifier is required
    p.expect(Identifier);

    while p.at(Comma) && !p.eof() {
        p.eat(Comma);

        p.expect(Identifier);
    }

    p.expect(RBracket);
}
