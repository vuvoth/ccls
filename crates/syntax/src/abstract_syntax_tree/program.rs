//! Program root and its top-level clauses: pragma, include, main component.

use parser::token_kind::TokenKind;
use parser::token_kind::TokenKind::*;
use rowan::ast::{support, AstNode};

use crate::node::{CircomLanguage, SyntaxNode};

use super::definition::{AstBusDef, AstFunctionDef, AstTemplateDef, AstTemplateName};
use super::expression::AstExpression;
use super::name::{AstIdentifier, Named};

// --- pragma / version --------------------------------------------------------

ast_node!(AstVersion, Version);
ast_node!(AstPragma, Pragma);

impl AstPragma {
    pub fn version(&self) -> Option<AstVersion> {
        support::child(self.syntax())
    }
}

// --- string / include --------------------------------------------------------

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

// --- main component ----------------------------------------------------------

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

// --- program root ------------------------------------------------------------

ast_node!(AstCircomProgram, CircomProgram);

impl AstCircomProgram {
    pub fn pragma(&self) -> Option<AstPragma> {
        support::child(self.syntax())
    }
    pub fn libs(&self) -> Vec<AstInclude> {
        support::children(self.syntax()).collect()
    }
    /// The include path strings (`"…"` stripped) declared in this program.
    pub fn include_paths(&self) -> Vec<String> {
        self.libs()
            .into_iter()
            .filter_map(|inc| inc.lib().map(|l| l.value()))
            .collect()
    }
    pub fn template_list(&self) -> Vec<AstTemplateDef> {
        support::children(self.syntax()).collect()
    }
    pub fn bus_list(&self) -> Vec<AstBusDef> {
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
        let wanted_name = wanted.identifier()?;
        self.template_list().into_iter().find(|t| {
            t.identifier()
                .is_some_and(|id| id.text() == wanted_name.text())
        })
    }
}
