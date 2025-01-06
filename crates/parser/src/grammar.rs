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

/**
 * parse circom program
 */

pub mod entry {

    use crate::token_kind::TokenKind;

    use super::*;

    pub fn circom_program(p: &mut Parser) {
        let m = p.open();

        while p.at_any(&[
            TokenKind::BlockComment,
            TokenKind::CommentLine,
            TokenKind::EndLine,
            TokenKind::WhiteSpace,
        ]) {
            p.skip();
        }

        while !p.eof() {
            match p.current() {
                Pragma => pragma::pragma(p),
                TemplateKw => template::template(p),
                IncludeKw => include::include(p),
                ComponentKw => main_component::main_component(p),
                FunctionKw => function::function_parse(p),
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
