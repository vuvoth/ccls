use parser::token_kind::TokenKind::*;

use crate::syntax_node::CircomLanguage;
use crate::syntax_node::SyntaxNode;
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;

use super::ast::AstBlock;
use super::ast::AstComponentDecl;
use super::ast::AstIdentifier;
use super::ast::AstInputSignalDecl;
use super::ast::AstOutputSignalDecl;
use super::ast::AstParameterList;
use super::ast::AstSignalDecl;
use super::ast::AstStatementList;

ast_node!(AstTemplateName, TemplateName);

ast_node!(AstTemplateDef, TemplateDef);

impl AstTemplateName {
    pub fn name(&self) -> Option<AstIdentifier> {
        self.syntax().children().find_map(AstIdentifier::cast)
    }
    pub fn same_name<M: AstNode<Language = CircomLanguage>>(&self, other: &M) -> bool {
        self.syntax().text() == other.syntax().text()
    }
}

impl AstTemplateDef {
    pub fn name(&self) -> Option<AstTemplateName> {
        self.syntax.children().find_map(AstTemplateName::cast)
    }
    pub fn func_body(&self) -> Option<AstBlock> {
        self.syntax.children().find_map(AstBlock::cast)
    }
    pub fn parameter_list(&self) -> Option<AstParameterList> {
        self.syntax().children().find_map(AstParameterList::cast)
    }
    pub fn statements(&self) -> Option<AstStatementList> {
        if let Some(body) = self.func_body() {
            return body.statement_list();
        }
        None
    }

    pub fn find_input_signal(&self, name: &str) -> Option<AstInputSignalDecl> {
        if let Some(statements) = self.statements() {
            for input_signal in statements.find_children::<AstInputSignalDecl>() {
                if let Some(signal_name) = input_signal.name() {
                    if signal_name.syntax().text() == name {
                        return Some(input_signal);
                    }
                }
            }
        }
        None
    }

    pub fn find_output_signal(&self, name: &str) -> Option<AstOutputSignalDecl> {
        if let Some(statements) = self.statements() {
            for input_signal in statements.find_children::<AstOutputSignalDecl>() {
                if let Some(signal_name) = input_signal.name() {
                    if signal_name.syntax().text() == name {
                        return Some(input_signal);
                    }
                }
            }
        }
        None
    }

    pub fn find_internal_signal(&self, name: &str) -> Option<AstSignalDecl> {
        if let Some(statements) = self.statements() {
            for signal in statements.find_children::<AstSignalDecl>() {
                if let Some(signal_name) = signal.name() {
                    if signal_name.syntax().text() == name {
                        return Some(signal);
                    }
                }
            }
        }
        None
    }

    pub fn find_component(&self, name: &str) -> Option<AstComponentDecl> {
        if let Some(statements) = self.statements() {
            for component in statements.find_children::<AstComponentDecl>() {
                if let Some(component_identifier) = component.component_identifier() {
                    if let Some(component_name) = component_identifier.name() {
                        if component_name.syntax().text() == name {
                            return Some(component);
                        }
                    }
                }
            }
        }
        None
    }
}
