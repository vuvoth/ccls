use crate::parser::Parser;
use crate::token_kind::TokenKind::*;

mod block;
mod bus;
mod declaration;
mod definition;
mod expression;
mod function;
mod include;
mod list;
mod main_component;
mod pragma;
mod statement;
mod template;

// Top-level program parsing.

pub mod entry {

    use super::*;

    /// Parse a whole circom program (grammar: `ParseAst`).
    ///
    /// Structure: `pragma* include* (template | function | main)*` in the common case, but pragmas
    /// and includes are also accepted interspersed with definitions (circom permits
    /// pragma/include/template/function/main in any order). Trivia between constructs is skipped by
    /// `Parser::current`/`at`. An unexpected token is recovered with an error rather than panicking.
    pub fn circom_program(p: &mut Parser) {
        let m = p.open();

        // Leading pragma*/include* (the conventional prefix) parsed first to keep the common path
        // linear; interspersed pragmas/includes are then handled inside the main loop below.
        while p.at(PragmaKw) {
            pragma::pragma(p);
        }

        while p.at(IncludeKw) {
            include::include(p);
        }

        // definitions* + optional main component, with pragma/include still accepted anywhere.
        while !p.eof() {
            match p.current() {
                TemplateKw => template::template(p),
                FunctionKw => function::function_parse(p),
                BusKw => bus::bus_definition(p),
                ComponentKw => main_component::main_component(p),
                PragmaKw => pragma::pragma(p),
                IncludeKw => include::include(p),
                // any other token is invalid at the top level
                _ => p.advance_with_error(),
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
