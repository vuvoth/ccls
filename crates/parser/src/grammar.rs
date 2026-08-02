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

// Top-level program parsing.

pub mod entry {

    use super::*;

    /// Parse a whole circom program (grammar: `ParseAst`).
    ///
    /// Structure: `pragma* include* (template | function | main)*`. Pragmas precede includes,
    /// which precede definitions, mirroring the official grammar. Trivia between constructs is
    /// skipped by `Parser::current`/`at`. An out-of-order or unexpected token is recovered with an
    /// error rather than panicking.
    pub fn circom_program(p: &mut Parser) {
        let m = p.open();

        // pragma*
        while p.at(PragmaKw) {
            pragma::pragma(p);
        }

        // include*
        while p.at(IncludeKw) {
            include::include(p);
        }

        // definitions* + optional main component
        while !p.eof() {
            match p.current() {
                TemplateKw => template::template(p),
                FunctionKw => function::function_parse(p),
                ComponentKw => main_component::main_component(p),
                other => p.advance_with_error(&format!("invalid top-level token {:?}", other)),
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
