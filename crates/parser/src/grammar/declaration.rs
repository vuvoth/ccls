use super::{
    expression::expression,
    list::{tuple_expression, tuple_identifier},
};
use crate::{parser::Parser, token_kind::TokenKind::*};

// [N][M-1]
fn array(p: &mut Parser) -> bool {
    let is_array = p.at(LBracket);

    while p.at(LBracket) {
        let array_marker = p.open();
        p.expect(LBracket);
        expression(p);
        p.expect(RBracket);
        p.close(array_marker, ArrayQuery);
    }

    is_array
}

/*
"signal" --> None
"signal input" --> Some(true)
"signal output" --> Some(false)
*/
fn signal_header(p: &mut Parser) -> Option<bool> {
    let m = p.open();
    p.expect(SignalKw);

    let res = if p.at(InputKw) {
        Some(true)
    } else if p.at(OutputKw) {
        Some(false)
    } else {
        None
    };

    if res.is_some() {
        p.advance();
    }

    // signal tags
    // {tag1, tag2, tag2}
    // TODO: support list of tags
    if p.at(LCurly) {
        p.expect(Identifier);
        p.expect(RCurly);
    }

    p.close(m, SignalHeader);
    res
}

pub(crate) fn var_init(p: &mut Parser) {
    // name of variable
    p.expect(Identifier);

    // eg: [N - 1][M]
    array(p);

    // assign for variable
    // eg: = 10
    if p.at_var_assign() {
        p.advance();
        expression(p);
    }
}

// eg: in[N - 1] <== c.in;
pub(crate) fn signal_init(p: &mut Parser, assign_able: bool) {
    // name of signal
    p.expect(Identifier);

    // eg: [N][M-1]
    array(p);

    // assign for  intermediate and outputs signals
    // eg: <== Multiplier2().out
    if assign_able && p.at_inline_assign_signal() {
        p.advance();
        expression(p);
    }
}

/**
 * Declaration := "var" (SimpleSymbol, ..., SimpleSymbol) TupleInitialization |
 *                "var" iden1 = init1, iden2 = init2, iden3         
 */
pub(super) fn var_declaration(p: &mut Parser) {
    let m = p.open();
    p.expect(VarKw);

    // tuple of variables
    // eg: var (in1, in2, in3) = (1, 2, 3);
    if p.at(LParen) {
        tuple_identifier(p);
        if p.at_var_assign() {
            p.advance();
            expression(p);
        }
    } else {
        // list of variables
        // var in1[N], in2 = 5;
        var_init(p);
        while p.at(Comma) && !p.eof() {
            p.skip();
            var_init(p);
        }
    }

    p.close(m, VarDecl);
}

/*
* signal are immutable (can not modify after init value)
* can not initialize value for input signal
* since circom 2.0.4, it is also allowed to initialize
intermediate and outputs signals right after their declaration
*/
pub(super) fn signal_declaration(p: &mut Parser) {
    // TODO: can we remove that?
    if !p.at(SignalKw) {
        p.advance_with_error("Signal error");
        return;
    }

    let m = p.open();
    let io_signal = signal_header(p);
    let assign_able = io_signal != Some(true);

    // tuple of signal
    // eg: signal (in1, in2, in3) <== tuple_value;
    if p.at(LParen) {
        tuple_identifier(p);
        // can not assign for input signal
        if assign_able && p.at_inline_assign_signal() {
            p.advance();
            expression(p);
        }
    } else {
        // list of signals
        // signal in1[N], in2 <== signal_value;
        signal_init(p, assign_able);
        while p.at(Comma) && !p.eof() {
            p.skip();
            signal_init(p, assign_able);
        }
    }

    let close_kind = match io_signal {
        Some(true) => InputSignalDecl,
        Some(false) => OutputSignalDecl,
        None => SignalDecl,
    };

    p.close(m, close_kind);
}

/*
* initialization in the definition of arrays of components is not allowed
*/
pub(super) fn component_declaration(p: &mut Parser) {
    let m = p.open();
    p.expect(ComponentKw);

    // TODO: why do we need `ComponentIdentifier` kind here?
    let m_c = p.open();
    p.expect(Identifier);
    p.close(m_c, ComponentIdentifier);

    // support array component
    // eg: comp[N - 1][10]
    let is_array = array(p);

    // do not assign for array components
    if !is_array && p.at(Assign) {
        p.expect(Assign);

        // TODO: support `parallel` tag
        // eg: component comp = parallel NameTemplate(...){...}

        // template name
        let m_c = p.open();
        p.expect(Identifier);
        p.close(m_c, TemplateName);

        // template params
        let parameter_marker = p.open();
        tuple_expression(p);
        p.close(parameter_marker, Call);
    }

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
