use list::tuple_expression;

use crate::parser::Marker;
use crate::token_kind::TokenKind;

use super::*;

// Binding power for ternary branches. The ternary `?:` is the loosest operator and is handled
// outside the Pratt loop (in `circom_expression`). Per the grammar (`Expression13: E12 ? E12 : E12`)
// both the condition and the branches are at the `||` level (`Expression12`): they may contain
// `||` and anything tighter, but NOT a nested ternary.
const MIN_BINDING_POWER: u16 = 0;
const TERNARY_BRANCH_BP: u16 = crate::token_kind::BP_BOOL_OR;

/// Parse a full expression, wrapping it in an `Expression` node.
/// (grammar: `ParseExpression` → ternary / binary / atom)
pub(super) fn expression(p: &mut Parser) {
    let m = p.open();
    circom_expression(p);
    p.close(m, Expression);
}

/// Parse an expression that may be a ternary `cond ? a : b`, or a plain binary/atom.
/// (grammar: `Expression13 = Expression12 "?" Expression12 ":" Expression12`)
fn circom_expression(p: &mut Parser) {
    if let Some(cond) = expression_rec(p, MIN_BINDING_POWER) {
        if p.at(MarkQuestion) {
            ternary_conditional(p, cond);
        }
    }
}

/// `cond ? if_true : if_false`. The branches are parsed at the `||` level so a nested ternary is
/// rejected (matching the grammar — `a ? b : c ? d : e` is a syntax error in circom).
///
/// The whole form is wrapped in a single `TenaryConditional` node. (A separate `Condition` node
/// is not emitted: the event/marker model cannot reliably double-wrap an already-parsed operand,
/// and circom's own AST models this as one `InlineSwitchOp`.)
fn ternary_conditional(p: &mut Parser, cond: Marker) {
    // <condition> ? <if_true> : <if_false>  — wrap the already-parsed condition in the node.
    let m = p.open_before(cond);

    p.expect(MarkQuestion);

    let if_true = p.open();
    expression_rec(p, TERNARY_BRANCH_BP);
    p.close(if_true, Expression);

    p.expect(Colon);

    let if_false = p.open();
    expression_rec(p, TERNARY_BRANCH_BP);
    p.close(if_false, Expression);

    p.close(m, TenaryConditional);
}

/// Precedence-climbing (Pratt) core. Returns the marker bounding the parsed expression, or
/// `None` if no atom could be parsed.
///
/// `min_bp` is the minimum infix left-binding-power required to continue: an operator is pulled
/// in only when `lbp >= min_bp`. Left-associative operators recurse with `rbp = lbp + 1`, so a
/// following same-precedence operator is NOT absorbed into the right operand (e.g.
/// `a - b - c` groups as `(a - b) - c`).
fn expression_rec(p: &mut Parser, min_bp: u16) -> Option<Marker> {
    let mut lhs = parse_prefix_or_atom(p)?;

    loop {
        let kind = p.current();

        if let Some((lbp, rbp)) = kind.infix() {
            if lbp < min_bp {
                break;
            }
            let m = p.open_before(lhs);
            p.advance(); // consume the infix operator
            expression_rec(p, rbp); // right operand at `rbp` (controls associativity)
            lhs = p.close(m, kind);
        } else if let Some(postfix_bp) = kind.postfix() {
            if postfix_bp < min_bp {
                break;
            }
            match parse_postfix(p, lhs, kind) {
                Some(new_lhs) => lhs = new_lhs,
                None => break, // error already reported; stop
            }
        } else {
            break;
        }
    }

    Some(lhs)
}

/// Parse a prefix expression (`!`, `~`, `-`) or fall through to an atom.
/// (grammar: `Expression2 = PrefixOpTier<ParseExpressionPrefixOpcode, Expression1>`)
fn parse_prefix_or_atom(p: &mut Parser) -> Option<Marker> {
    let kind = p.current();
    if let Some(prefix_bp) = kind.prefix() {
        let m = p.open();
        p.advance(); // consume the prefix operator
        expression_rec(p, prefix_bp); // operand at the prefix binding power
        Some(p.close(m, kind))
    } else {
        expression_atom(p)
    }
}

/// Parse a single postfix operator (call `()`, index `[]`, member `.`). Returns the new lhs
/// marker, or `None` on an unexpected token (error already reported). Chaining is handled by the
/// caller's loop. (grammar: `Expression1` postfix forms)
fn parse_postfix(p: &mut Parser, lhs: Marker, kind: TokenKind) -> Option<Marker> {
    let m = p.open_before(lhs);
    match kind {
        LParen => {
            // function/template call: name(arg, ...)
            tuple_expression(p);
            Some(p.close(m, Call))
        }
        LBracket => {
            // array subscript: arr[expr]
            p.expect(LBracket);
            expression(p);
            p.expect(RBracket);
            Some(p.close(m, ArrayQuery))
        }
        Dot => {
            // member / component-signal access: obj.field
            p.expect(Dot);
            p.expect(Identifier);
            Some(p.close(m, ComponentCall))
        }
        _ => {
            p.advance_with_error(&format!("expected a postfix token, found {:?}", kind));
            None
        }
    }
}

/// Parse an expression atom: an identifier, a numeric literal (decimal or hex), or a
/// parenthesized expression (which may itself contain a ternary).
/// (grammar: `Expression0`)
fn expression_atom(p: &mut Parser) -> Option<Marker> {
    let kind = p.current();
    match kind {
        Number | HexNumber | Identifier => {
            let m = p.open();
            p.advance();
            Some(p.close(m, ExpressionAtom))
        }
        LParen => {
            // ( <expression> )  — a parenthesized expression is a full expression, so it may
            // contain a ternary: `(a ? b : c)`. Use `circom_expression` (ternary-capable) rather
            // than `expression_rec` (which stops at `?`).
            let m = p.open();
            p.expect(LParen);
            circom_expression(p);
            p.expect(RParen);
            Some(p.close(m, Expression))
        }
        _ => {
            p.advance_with_error("invalid token");
            None
        }
    }
}
