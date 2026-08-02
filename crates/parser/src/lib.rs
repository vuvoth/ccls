//! Circom language lexer, event-based parser, and grammar.
//!
//! The grammar rules follow the official circom compiler grammar
//! (`iden3/circom` → `parser/src/lang.lalrpop`). Expressions are parsed with Pratt /
//! precedence climbing (see `token_kind::TokenKind::{infix,prefix,postfix}`).

#![warn(clippy::style, clippy::complexity, clippy::perf, clippy::suspicious)]
#![deny(clippy::correctness)]

pub mod event;
pub mod grammar;
pub mod lexer;
pub mod parser;
pub mod token_kind;
