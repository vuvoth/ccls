use crate::parser::Parser;
use crate::token_kind::TokenKind::*;

mod block;
mod declaration;
mod expression;
mod function;
mod include;
mod list;
mod main_component;
mod pragma;
mod statement;
mod template;

pub mod entry {

    use super::*;

    /// Parses a full Circom program from top-level constructs.
    ///
    /// This function handles parsing of all valid top-level elements in a Circom file,
    /// such as pragmas, templates, includes, component instantiations, and function
    /// definitions. Invalid tokens are flagged with an error.
    ///
    /// Grammar:
    /// * `<CircomProgram>` ::= { `<Pragma>` | `<Include>` | `<Template>` | `<Component>` | `<Function>` }
    ///
    /// Example:
    /// ```ignore
    /// pragma circom 2.0.0;
    /// include "utils.circom";
    ///
    /// template Adder(a, b) {
    ///     signal input a;
    ///     signal input b;
    ///     signal output c;
    ///     c <== a + b;
    /// }
    ///
    /// function double(x) -> y {
    ///     y = x * 2;
    /// }
    ///
    /// component main = Adder(1, 2);
    /// ```
    /// * `<Pragma>`: `pragma circom 2.0.0;`
    /// * `<Include>`: `include "utils.circom";`
    /// * `<Template>`: A circuit blueprint, e.g. `template Adder(...) { ... }`
    /// * `<Function>`: A reusable logic function, e.g. `function double(...) -> ... { ... }`
    /// * `<Component>`: An instance of a template, e.g. `component main = Adder(...);`
    ///
    pub fn circom_program(p: &mut Parser) {
        let m = p.open();

        while !p.eof() {
            match p.current() {
                PragmaKw => pragma::pragma(p),
                TemplateKw => template::template(p),
                IncludeKw => include::include(p),
                ComponentKw => main_component::main_component(p),
                FunctionKw => function::function(p),
                _ => p.advance_with_error("invalid token"),
            }
        }

        p.close(m, CircomProgram);
    }

    pub enum Scope {
        Block,
        CircomProgram,
        Pragma,
        Template,
    }

    impl Scope {
        pub fn parse(self, p: &mut Parser) {
            match self {
                Self::Block => block::block(p),
                Self::CircomProgram => circom_program(p),
                Self::Pragma => pragma::pragma(p),
                Self::Template => template::template(p),
            }
        }
    }
}
