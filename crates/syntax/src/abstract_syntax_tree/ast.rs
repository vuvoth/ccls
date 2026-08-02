use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::syntax_node::CircomLanguage;
use crate::syntax_node::SyntaxNode;
use parser::token_kind::TokenKind;

use super::expression::AstExpression;
use super::template::{AstTemplateDef, AstTemplateName};

/// A node that declares a nameable symbol and exposes its leaf identifier token — the `a` in
/// `signal input a;`, the `T` in `template T() {}`. Implementing it lets a declaration be looked up
/// generically via [`AstStatementList::find`] and compared by name without the caller knowing the
/// node's identifier-chain shape (`signal_identifier().name()` vs `component_identifier().name()`).
pub trait Named: AstNode<Language = CircomLanguage> {
    fn identifier(&self) -> Option<AstIdentifier>;
}

// --- signal declarations -----------------------------------------------------

ast_node!(AstSignalHeader, SignalHeader);
ast_node!(AstInputSignalDecl, InputSignalDecl);
ast_node!(AstOutputSignalDecl, OutputSignalDecl);
ast_node!(AstSignalDecl, SignalDecl);

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

// --- statements / blocks -----------------------------------------------------

ast_node!(AstStatementList, StatementList);

impl AstStatementList {
    /// All direct children that cast to `N`. The grammar emits each statement as its concrete node
    /// kind (not a generic `Statement` node), so statements are queried by concrete type.
    pub fn find_children<N: AstNode<Language = CircomLanguage>>(&self) -> Vec<N> {
        support::children(self.syntax()).collect()
    }

    /// The first child of type `N` whose declared name equals `name`.
    pub fn find<N: Named>(&self, name: &str) -> Option<N> {
        self.find_children::<N>()
            .into_iter()
            .find(|n| n.identifier().is_some_and(|id| id.syntax().text() == name))
    }
}

ast_node!(AstBlock, Block);

impl AstBlock {
    pub fn statement_list(&self) -> Option<AstStatementList> {
        support::child(self.syntax())
    }
}

// --- pragma / version --------------------------------------------------------

ast_node!(AstVersion, Version);
ast_node!(AstPragma, Pragma);

impl AstPragma {
    pub fn version(&self) -> Option<AstVersion> {
        support::child(self.syntax())
    }
}

// --- identifiers / names / params -------------------------------------------

ast_node!(AstParameterList, ParameterList);

impl AstParameterList {
    pub fn parameters(&self) -> Vec<AstIdentifier> {
        support::children(self.syntax()).collect()
    }
}

ast_node!(AstComplexIdentifier, ComplexIdentifier);

impl AstComplexIdentifier {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstIdentifier, Identifier);

// --- functions / program -----------------------------------------------------

ast_node!(AstFunctionName, FunctionName);

ast_node!(AstFunctionDef, FunctionDef);

impl AstFunctionDef {
    pub fn body(&self) -> Option<AstBlock> {
        support::child(self.syntax())
    }
    pub fn function_name(&self) -> Option<AstFunctionName> {
        support::child(self.syntax())
    }
    pub fn parameter_list(&self) -> Option<AstParameterList> {
        support::child(self.syntax())
    }
    pub fn statements(&self) -> Option<AstStatementList> {
        self.body().and_then(|b| b.statement_list())
    }
}

ast_node!(AstCircomProgram, CircomProgram);

impl AstCircomProgram {
    pub fn pragma(&self) -> Option<AstPragma> {
        support::child(self.syntax())
    }
    pub fn libs(&self) -> Vec<AstInclude> {
        support::children(self.syntax()).collect()
    }
    pub fn template_list(&self) -> Vec<AstTemplateDef> {
        support::children(self.syntax()).collect()
    }
    pub fn function_list(&self) -> Vec<AstFunctionDef> {
        support::children(self.syntax()).collect()
    }
    pub fn main_component(&self) -> Option<AstMainComponent> {
        support::child(self.syntax())
    }
    /// The template definition whose name matches `wanted`, compared by source text.
    pub fn get_template_by_name(&self, wanted: &AstTemplateName) -> Option<AstTemplateDef> {
        let wanted = wanted.identifier()?;
        self.template_list()
            .into_iter()
            .find(|t| t.identifier().is_some_and(|id| id.text() == wanted.text()))
    }
}

// --- component call / string / include / main --------------------------------

ast_node!(AstComponentCall, ComponentCall);

impl AstComponentCall {
    pub fn component_name(&self) -> Option<AstComplexIdentifier> {
        support::child(self.syntax())
    }
    pub fn signal(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstCircomString, CircomString);

impl AstCircomString {
    /// String contents with exactly the surrounding `"` quotes stripped (never panics on a
    /// short/malformed token).
    pub fn value(&self) -> String {
        let text = self.syntax().text().to_string();
        match text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
            Some(inner) => inner.to_string(),
            None => text,
        }
    }
}

ast_node!(AstInclude, Include);

impl AstInclude {
    pub fn lib(&self) -> Option<AstCircomString> {
        support::child(self.syntax())
    }
}

ast_node!(AstMainComponent, MainComponent);

impl AstMainComponent {
    /// The instantiation on the right of `=` (e.g. `Surface(4, 2)`).
    pub fn instantiation(&self) -> Option<AstExpression> {
        support::child(self.syntax())
    }
    /// Names in the optional `{public […]}` clause (empty when absent).
    pub fn public_signals(&self) -> Vec<AstIdentifier> {
        support::children(self.syntax()).collect()
    }
}
