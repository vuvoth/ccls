use super::{
    expression::{tuple, tuple_init},
    *,
};

// "signal" --> None
// "signal input" --> Some(true)
// "signal output" --> Some(false)
fn signal_header(p: &mut Parser) -> Option<bool> {
    let mut res = None;
    let m = p.open();
    p.expect(SignalKw);
    if p.at_any(&[InputKw, OutputKw]) {
        if p.at(InputKw) {
            res = Some(true);
        } else {
            res = Some(false);
        }
        p.advance();

        if p.at(LCurly) {
            p.expect(Identifier);
            p.expect(RCurly);
        }
    }
    p.close(m, SignalHeader);
    res
}

/**
 * Declaration := "var" (SimpleSymbol, ..., SimpleSymbol) TupleInitialization |
 *               
 *             
 */
pub(super) fn var_declaration(p: &mut Parser) {
    let m = p.open();
    p.expect(VarKw);

    if p.at(LParen) {
        tuple(p);
        if p.at(Assign) {
            tuple_init(p);
        }
    } else {
        p.expect(Identifier);
        if p.at(Assign) {
            p.expect(Assign);
            expression::expression(p);
        }
        // list of var
        while p.at(Comma) && !p.eof() {
            p.expect(Comma);
            p.expect(Identifier);
            if p.at(Assign) {
                p.expect(Assign);
                expression::expression(p);
            }
        }
    }
    p.close(m, VarDecl);
}

pub(super) fn signal_declaration(p: &mut Parser) {
    if !p.at(SignalKw) {
        p.advance_with_error("Signal error");
        return;
    }

    let m = p.open();
    let io_signal = signal_header(p);

    if p.at(LParen) {
        tuple(p);
        if p.at_any(&[Assign, RAssignSignal, RAssignConstraintSignal]) {
            tuple_init(p);
        }
    } else {
        p.expect(Identifier);
        // list of var
        while p.at(Comma) && !p.eof() {
            p.skip();
            p.expect(Identifier);
        }
    }

    if let Some(is_input) = io_signal {
        if is_input {
            p.close(m, InputSignalDecl);
        } else {
            p.close(m, OutputSignalDecl);
        }
    } else {
        p.close(m, SignalDecl);
    }
}

pub(super) fn component_declaration(p: &mut Parser) {
    let m = p.open();
    p.expect(ComponentKw);
    let m_c = p.open();
    p.expect(Identifier);
    p.close(m_c, ComponentIdentifier);

    p.expect(Assign);
    let m_c = p.open();
    p.expect(Identifier);
    p.close(m_c, TemplateName);
    p.expect(LParen);

    if p.at(Identifier) {
        expression::expression(p);
        while !p.at(RParen) && !p.eof() {
            p.expect(Comma);
            expression::expression(p);
        }
    }

    p.expect(RParen);

    p.close(m, ComponentDecl);
}

pub(super) fn declaration(p: &mut Parser) {
    match p.current() {
        SignalKw => signal_declaration(p),
        VarKw => var_declaration(p),
        ComponentKw => component_declaration(p),
        _ => unreachable!(),
    }
}
