//! Block-level declarations: signals, variables, and components.

use parser::token_kind::TokenKind;
use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::syntax_node::{CircomLanguage, SyntaxNode};

use super::definition::AstTemplateName;
use super::name::{AstComplexIdentifier, AstIdentifier, Named};

// --- signal declarations -----------------------------------------------------

ast_node!(AstSignalHeader, SignalHeader);
ast_node!(AstInputSignalDecl, InputSignalDecl);
ast_node!(AstOutputSignalDecl, OutputSignalDecl);
ast_node!(AstSignalDecl, SignalDecl);

/// Implement `signal_identifier()` + `Named` for each signal-decl kind: they all expose their
/// declared name as a `ComplexIdentifier` whose leaf `Identifier` is the signal name.
macro_rules! impl_signal_identifier {
    ($ty:ty) => {
        impl $ty {
            pub fn signal_identifier(&self) -> Option<AstComplexIdentifier> {
                support::child(self.syntax())
            }
        }
        impl Named for $ty {
            fn identifier(&self) -> Option<AstIdentifier> {
                self.signal_identifier().and_then(|c| c.name())
            }
        }
    };
}
impl_signal_identifier!(AstInputSignalDecl);
impl_signal_identifier!(AstOutputSignalDecl);
impl_signal_identifier!(AstSignalDecl);

// --- variable / component declarations ---------------------------------------

ast_node!(AstVarDecl, VarDecl);

impl AstVarDecl {
    pub fn var_identifier(&self) -> Option<AstComplexIdentifier> {
        support::child(self.syntax())
    }
}
impl Named for AstVarDecl {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.var_identifier().and_then(|c| c.name())
    }
}

ast_node!(AstComponentDecl, ComponentDecl);

impl AstComponentDecl {
    /// The instantiated template, e.g. `Poseidon` in `component hash = Poseidon(2);`.
    pub fn template(&self) -> Option<AstTemplateName> {
        support::child(self.syntax())
    }
    pub fn component_identifier(&self) -> Option<AstComplexIdentifier> {
        support::child(self.syntax())
    }
}
impl Named for AstComponentDecl {
    fn identifier(&self) -> Option<AstIdentifier> {
        self.component_identifier().and_then(|c| c.name())
    }
}
