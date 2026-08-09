use super::{
    expression::expression,
    list::{paren_list, tuple_identifier},
    *,
};
use crate::parser::Parser;

// [N][M-1]
fn array(p: &mut Parser) -> bool {
    let is_array = p.at(LBracket);

    while p.at(LBracket) {
        p.expect(LBracket);
        expression(p);
        p.expect(RBracket);
    }

    is_array
}

/*
* eg: a, a[N], a[N][M - 1],...
*/
pub(crate) fn complex_identifier(p: &mut Parser) {
    let open_marker = p.open();

    // name
    p.expect(Identifier);

    // eg: [N - 1][M]
    array(p);

    p.close(open_marker, ComplexIdentifier);
}

/*
 * Parse a signal header (grammar: `SignalHeader`).
 *
 * Two keyword orders are accepted:
 *   signal (input|output)?   -> intermediate / input / output
 *   (input|output) signal    -> input / output
 * Optionally followed by a tag list `{ tag1, tag2, ... }` (grammar `ParseTagsList`).
 *
 * Returns `Some(true)` for input, `Some(false)` for output, `None` for intermediate.
 */
fn signal_header(p: &mut Parser) -> Option<bool> {
    let m = p.open();

    let result = match p.current() {
        // "signal" ("input"|"output")?
        SignalKw => {
            p.expect(SignalKw);
            match p.current() {
                InputKw => {
                    p.advance();
                    Some(true)
                }
                OutputKw => {
                    p.advance();
                    Some(false)
                }
                _ => None,
            }
        }
        // ("input"|"output") "signal"
        InputKw => {
            p.advance();
            p.expect(SignalKw);
            Some(true)
        }
        OutputKw => {
            p.advance();
            p.expect(SignalKw);
            Some(false)
        }
        _ => None,
    };

    // tag list: { tag1, tag2, ... }
    if p.at(LCurly) {
        tag_list(p);
    }

    p.close(m, SignalHeader);
    result
}

/// Parse a comma-separated tag list inside `{ ... }` (grammar: `ParseTagsList`).
/// Requires at least one identifier when the braces are present.
fn tag_list(p: &mut Parser) {
    p.expect(LCurly);
    if p.at(Identifier) {
        p.expect(Identifier);
        while p.at(Comma) && !p.eof() {
            p.skip();
            p.expect(Identifier);
        }
    }
    p.expect(RCurly);
}

/*
var_init does not include `var` keyword
eg: tmp = 10;
*/
pub(crate) fn var_init(p: &mut Parser) {
    // var identifier
    // eg: a[N]
    complex_identifier(p);

    // assign for variable
    // eg: = 10
    if p.at_var_assign() {
        p.advance();
        expression(p);
    }
}

// eg: in[N - 1] <== c.in;
pub(crate) fn signal_init(p: &mut Parser, assign_able: bool) {
    // signal identifier
    // eg: in[N]
    complex_identifier(p);

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
    // Accept `signal ...`, `input signal ...`, or `output signal ...` (grammar `SignalHeader`).
    if !p.at(SignalKw) && !p.at(InputKw) && !p.at(OutputKw) {
        // expected a signal declaration
        p.advance_with_error();
        return;
    }

    let m = p.open();
    let io_signal = signal_header(p);
    let assign_able = io_signal != Some(true);

    field_list(p, assign_able);

    let close_kind = match io_signal {
        Some(true) => InputSignalDecl,
        Some(false) => OutputSignalDecl,
        None => SignalDecl,
    };

    p.close(m, close_kind);
}

/// Comma-separated signal/field initializers, or the tuple form `(a, b, …)` (grammar
/// `SignalSymbol` list / `TupleInitialization`). Shared by `signal_declaration` and the bus-typed
/// field declaration.
///
/// `assign_able` is false for inputs (which cannot carry an initializer); when an inline signal
/// assignment (`<==` / `<--` / `=`) follows the tuple/list it is parsed as the initializer.
fn field_list(p: &mut Parser, assign_able: bool) {
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
}

/// Bus-typed field declaration (grammar: `BusHeader`). Reached only via `input_or_output`, i.e.
/// with a leading `input`/`output` keyword:
///
/// ```text
/// <input|output> <BusType> ("("<expr-list>")")? {tags}? <fieldName…>
/// ```
///
/// The field name(s) are a `SignalSymbol` parsed by `field_list`. Reuses `paren_list` for the bus
/// instantiation args, `tag_list` for tags, and `field_list` for the trailing name(s). Gaps 15/16/21.
pub(super) fn bus_signal_declaration(p: &mut Parser) {
    let m = p.open();

    // Leading wire direction. `input_or_output` guarantees the current token is `input`/`output`;
    // the `_ => None` arm keeps the match exhaustive (defensive — unreachable in practice).
    let io: Option<bool> = match p.current() {
        InputKw => {
            p.advance();
            Some(true)
        }
        OutputKw => {
            p.advance();
            Some(false)
        }
        _ => None,
    };

    // Bus type identifier.
    p.expect(Identifier);

    // Optional bus instantiation args `(expr, …)` (grammar: `("("<Listable?>")")?`).
    if p.at(LParen) {
        paren_list(p);
    }

    // Optional tag list `{ tag, … }`.
    if p.at(LCurly) {
        tag_list(p);
    }

    let assign_able = io != Some(true);
    field_list(p, assign_able);

    let close_kind = match io {
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

    // `component c[N]` — the `[` follows the name. Detect before parsing so we can flag an
    // initializer on an array component, which circom forbids.
    let is_array_component = p.nth(1) == LBracket;

    complex_identifier(p);

    if p.at(Assign) {
        if is_array_component {
            p.error_report("array components cannot be initialized".to_string());
        }
        p.expect(Assign);

        let m_c = p.open();
        p.expect(Identifier);
        p.close(m_c, TemplateName);

        let parameter_marker = p.open();
        paren_list(p);
        p.close(parameter_marker, Call);
    }

    p.close(m, ComponentDecl);
}

pub(super) fn declaration(p: &mut Parser) {
    match p.current() {
        SignalKw => signal_declaration(p),
        InputKw | OutputKw => input_or_output(p),
        VarKw => var_declaration(p),
        ComponentKw => component_declaration(p),
        // any other token — expected a declaration keyword
        _ => p.advance_with_error(),
    }
}

/// Parse an `input`/`output`-led declaration, dispatching between a bus-typed field
/// (`input B b;`, grammar `BusHeader`) and an ordinary signal declaration (`input signal a;`)
/// via non-emitting lookahead on the second token. The single source of this decision so block
/// scope and `for`-init scope cannot diverge.
pub(super) fn input_or_output(p: &mut Parser) {
    if p.nth(1) != SignalKw {
        bus_signal_declaration(p);
    } else {
        signal_declaration(p);
    }
}
