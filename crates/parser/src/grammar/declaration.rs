use super::{
    expression::{tuple, tuple_init},
    *,
};


fn signal_header(p: &mut Parser) {
    let m = p.open();
    p.expect(SignalKw);
    if p.at_any(&[InputKw, OutputKw]) {
        p.advance();
    }
    p.close(m, SignalHeader);
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
        if p.at_any(&[Assign, RAssignSignal, RAssignConstraintSignal]) {
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
    p.close(m, VarKw);
}

pub(super) fn signal_declaration(p: &mut Parser) {
    if !p.at(SignalKw) {
        p.advance_with_error("Signal error");
        return;
    }

    let m = p.open();
    signal_header(p);
   
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
    p.close(m, SignalKw);
}


pub(super) fn declaration(p: &mut Parser) {
    match p.current().kind {
        SignalKw => signal_declaration(p),
        VarKw => var_declaration(p),
        _ => unreachable!()
    }
}