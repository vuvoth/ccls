use list::{expression_list, paren_list};

use crate::parser::Marker;
use crate::token_kind::TokenKind;

use super::*;

// Binding power for ternary branches. The ternary `?:` is the loosest operator and is handled
// outside the Pratt loop (in `circom_expression`). Per the grammar (`Expression13: E12 ? E12 : E12`)
// both the condition and the branches are at the `||` level (`Expression12`): they may contain
// `||` and anything tighter, but NOT a nested ternary.
const MIN_BINDING_POWER: u16 = 0;
const TERNARY_BRANCH_BP: u16 = crate::token_kind::BP_BOOL_OR;

/// Maximum expression-nesting depth. Generous for real circuits (which rarely exceed a few dozen
/// levels) but small enough that recursion can never approach the OS stack limit (~8 MiB). Past
/// this the parser stops recursing and reports an error, defending the language server against
/// adversarial deeply-nested input (`[[[...]]]`, `parallel (parallel …)`, nested parens).
const MAX_EXPR_DEPTH: u32 = 256;

/// Parse a full expression, wrapping it in an `Expression` node.
/// (grammar: `ParseExpression` → ternary / binary / atom)
pub(super) fn expression(p: &mut Parser) {
    let m = p.open();
    circom_expression(p);
    p.close(m, Expression);
}

/// Parse an expression that may be a ternary `cond ? a : b`, a parallel-wrapped expression, or a
/// plain binary/atom. Returns the marker bounding the parsed expression's outermost node, or `None`
/// if no atom could be parsed (an error was already reported).
///
/// (grammar: `Expression14 = "parallel" <ParseExpression1>` wraps the `||`/ternary level — no
/// nested `parallel`; `Expression13 = Expression12 "?" Expression12 ":" Expression12`.)
///
/// The `ParallelKw` token doubles as the wrapping node kind (the same dual token+node pattern used
/// for prefix operators like `Sub`/`Not`).
fn circom_expression(p: &mut Parser) -> Option<Marker> {
    if p.at(ParallelKw) {
        let m = p.open();
        p.advance(); // consume `parallel`
                     // The wrapped operand (an `Expression` node, like ternary branches), for uniform access.
        let op = p.open();
        expression_ternary(p);
        p.close(op, Expression);
        Some(p.close(m, ParallelKw))
    } else {
        expression_ternary(p)
    }
}

/// `cond ? a : b` or a plain `||`-level expression. Nested `parallel` is rejected here (the atom
/// arms do not recognize `ParallelKw`), matching the grammar.
fn expression_ternary(p: &mut Parser) -> Option<Marker> {
    let cond = expression_rec(p, MIN_BINDING_POWER)?;
    if p.at(MarkQuestion) {
        Some(ternary_conditional(p, cond))
    } else {
        Some(cond)
    }
}

/// `cond ? if_true : if_false`. The branches are parsed at the `||` level so a nested ternary is
/// rejected (matching the grammar — `a ? b : c ? d : e` is a syntax error in circom).
///
/// The whole form is wrapped in a single `TernaryConditional` node (the event/marker model cannot
/// reliably double-wrap an already-parsed operand, and circom's own AST models this as one
/// `InlineSwitchOp`).
fn ternary_conditional(p: &mut Parser, cond: Marker) -> Marker {
    // <condition> ? <if_true> : <if_false>  — wrap the already-parsed condition in the node.
    let m = p.precede(cond);

    p.expect(MarkQuestion);

    let if_true = p.open();
    expression_rec(p, TERNARY_BRANCH_BP);
    p.close(if_true, Expression);

    p.expect(Colon);

    let if_false = p.open();
    expression_rec(p, TERNARY_BRANCH_BP);
    p.close(if_false, Expression);

    p.close(m, TernaryConditional)
}

/// Precedence-climbing (Pratt) core. Returns the marker bounding the parsed expression, or
/// `None` if no atom could be parsed.
///
/// `min_bp` is the minimum infix left-binding-power required to continue: an operator is pulled
/// in only when `lbp >= min_bp`. Left-associative operators recurse with `rbp = lbp + 1`, so a
/// following same-precedence operator is NOT absorbed into the right operand (e.g.
/// `a - b - c` groups as `(a - b) - c`).
///
/// Each call increments `p.depth` (decremented on return via the IIFE). Past `MAX_EXPR_DEPTH` it
/// stops recursing and reports an error, bounding the C-stack on adversarial deeply-nested input.
/// `fuel` cannot serve this role: it resets on every token emission, and recursive descent always
/// emits, so a progressing-but-deep parse would never trip it.
fn expression_rec(p: &mut Parser, min_bp: u16) -> Option<Marker> {
    p.depth += 1;
    let result = (|| {
        if p.depth > MAX_EXPR_DEPTH {
            p.error_report("expression nesting too deep".to_string());
            return None;
        }

        let mut lhs = parse_prefix_or_atom(p)?;

        loop {
            let kind = p.current();

            if let Some((lbp, rbp)) = kind.infix() {
                if lbp < min_bp {
                    break;
                }
                let m = p.precede(lhs);
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
    })();
    p.depth -= 1;
    result
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
    let m = p.precede(lhs);
    match kind {
        LParen => {
            // function/template call: name(arg, ...)
            paren_list(p);
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
            // expected a postfix token, found `kind`
            p.advance_with_error();
            None
        }
    }
}

/// Parse an expression atom: an identifier, a numeric literal (decimal or hex), the `_`
/// placeholder, a parenthesized expression (grouping or tuple), or an inline array literal.
/// (grammar: `Expression0` + `Expression1` inline-array/tuple forms)
fn expression_atom(p: &mut Parser) -> Option<Marker> {
    let kind = p.current();
    match kind {
        Number | HexNumber | Identifier | Underscore => {
            let m = p.open();
            p.advance();
            Some(p.close(m, ExpressionAtom))
        }
        LBracket => {
            // inline array literal: [a, b, …] (grammar Expression1: `"[" <Listable> "]"`, ≥1 elem).
            let m = p.open();
            p.expect(LBracket);
            expression_list(p);
            p.expect(RBracket);
            Some(p.close(m, InlineArray))
        }
        LParen => {
            // ( <expr> ) grouping, OR ( <expr>, <expr>, … ) tuple (≥2 elems, grammar Expression1).
            // The first element is parsed ternary-capable (`circom_expression`) so `(a ? b : c)`
            // groupings work; a following comma flips this into a `TupleExpr`.
            let m = p.open();
            p.expect(LParen);
            let first = circom_expression(p);
            if p.nth(0) == Comma {
                // tuple — wrap the first element in an `Expression` node so EVERY element is a
                // uniform `Expression` child (the remaining elements come pre-wrapped from
                // `expression_list`). `precede` wraps the already-parsed first operand in place.
                if let Some(f) = first {
                    let w = p.precede(f);
                    p.close(w, Expression);
                }
                p.eat(Comma); // now consume the separator (and any leading trivia)
                expression_list(p);
                p.expect(RParen);
                Some(p.close(m, TupleExpr))
            } else {
                // grouping — `( <expression> )`. Unchanged from the pre-tuple path.
                p.expect(RParen);
                Some(p.close(m, Expression))
            }
        }
        _ => {
            // invalid token
            p.advance_with_error();
            None
        }
    }
}
