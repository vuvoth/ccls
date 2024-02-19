use crate::syntax_node::{CircomLang, CircomLanguage};
pub use rowan::ast::{support, AstChildren, AstNode};
use rowan::{Language, SyntaxText};

use crate::{
    syntax_node::SyntaxNode,
    token_kind::{TokenKind, TokenKind::*},
};

macro_rules! ast_node {
    ($ast_name: ident, $kind: expr) => {
        #[derive(Debug, Clone)]
        pub struct $ast_name {
            syntax: SyntaxNode,
        }
        impl AstNode for $ast_name {
            type Language = CircomLanguage;
            fn can_cast(token_kind: TokenKind) -> bool {
                token_kind == $kind
            }

            fn cast(syntax: SyntaxNode) -> Option<Self>
            where
                Self: Sized,
            {
                if Self::can_cast(syntax.kind().into()) {
                    return Some(Self { syntax });
                }
                None
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

ast_node!(AstSignalHeader, SignalHeader);
ast_node!(AstInputSignalDecl, InputSignalDecl);
ast_node!(AstOutputSignalDecl, OutputSignalDecl);
ast_node!(AstSignalDecl, SignalDecl);

impl AstInputSignalDecl {
    pub fn signal_name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }

    pub fn same_name(&self, other: &SyntaxText) -> bool {
        if let Some(name) = self.signal_name() {
            return name.equal(other);
        }
        false
    }
}

impl AstOutputSignalDecl {
    pub fn signal_name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
impl AstSignalDecl {
    pub fn signal_name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
ast_node!(AstVarDecl, VarDecl);

impl AstVarDecl {
    pub fn variable_name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstComponentDecl, ComponentDecl);

impl AstComponentDecl {
    pub fn template(&self) -> Option<AstTemplateName> {
        support::child(self.syntax())
    }
    pub fn component_identifier(&self) -> Option<AstComponentIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstStatement, Statement);

ast_node!(AstStatementList, StatementList);

impl AstStatementList {
    pub fn statement_list(&self) -> AstChildren<AstStatement> {
        support::children(self.syntax())
    }

    pub fn input_signals(&self) -> Vec<AstInputSignalDecl> {
        self.syntax()
            .children()
            .filter_map(AstInputSignalDecl::cast)
            .collect()
    }

    pub fn output_signals(&self) -> Vec<AstOutputSignalDecl> {
        self.syntax()
            .children()
            .filter_map(AstOutputSignalDecl::cast)
            .collect()
    }

    pub fn internal_signals(&self) -> Vec<AstSignalDecl> {
        self.syntax()
            .children()
            .filter_map(AstSignalDecl::cast)
            .collect()
    }
    pub fn variables(&self) -> Vec<AstVarDecl> {
        self.syntax()
            .children()
            .filter_map(AstVarDecl::cast)
            .collect()
    }

    pub fn components(&self) -> Vec<AstComponentDecl> {
        self.syntax()
            .children()
            .filter_map(AstComponentDecl::cast)
            .collect()
    }
}

ast_node!(AstBlock, Block);
impl AstBlock {
    pub fn statement_list(&self) -> Option<AstStatementList> {
        support::child::<AstStatementList>(self.syntax())
    }
}

ast_node!(AstVersion, Version);
ast_node!(AstPragma, Pragma);

impl AstPragma {
    pub fn version(&self) -> Option<AstVersion> {
        support::child(self.syntax())
    }
}
ast_node!(AstParameterList, TokenKind::ParameterList);

ast_node!(AstIdentifier, Identifier);

impl AstIdentifier {
    pub fn equal(&self, other: &SyntaxText) -> bool {
        self.syntax().text() == *other
    }
}

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
    pub fn template_name(&self) -> Option<AstTemplateName> {
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

    pub fn find_input_signal(&self, name: &SyntaxText) -> Option<AstInputSignalDecl> {
        if let Some(statements) = self.statements() {
            for input_signal in statements.input_signals() {
                if let Some(signal_name) = input_signal.signal_name() {
                    if signal_name.equal(name) {
                        return Some(input_signal);
                    }
                }
            }
        }
        None
    }

    pub fn find_output_signal(&self, name: &SyntaxText) -> Option<AstOutputSignalDecl> {
        if let Some(statements) = self.statements() {
            for input_signal in statements.output_signals() {
                if let Some(signal_name) = input_signal.signal_name() {
                    if signal_name.equal(name) {
                        return Some(input_signal);
                    }
                }
            }
        }
        None
    }

    pub fn find_internal_signal(&self, name: &SyntaxText) -> Option<AstSignalDecl> {
        if let Some(statements) = self.statements() {
            for signal in statements.internal_signals() {
                if let Some(signal_name) = signal.signal_name() {
                    if signal_name.equal(&name) {
                        return Some(signal);
                    }
                }
            }
        }
        None
    }

    pub fn find_component(&self, name: &str) -> Option<AstComponentDecl> {
        if let Some(statements) = self.statements() {
            for component in statements.components() {
                if let Some(signal_name) = component.component_identifier() {
                    if let Some(component_name) = signal_name.name() {
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

ast_node!(AstFunctionName, FunctionName);

ast_node!(AstFunctionDef, FunctionDef);

impl AstFunctionDef {
    pub fn body(&self) -> Option<AstBlock> {
        self.syntax().children().find_map(AstBlock::cast)
    }

    pub fn function_name(&self) -> Option<AstFunctionName> {
        self.syntax().children().find_map(AstFunctionName::cast)
    }

    pub fn argument_list(&self) -> Option<AstParameterList> {
        self.syntax().children().find_map(AstParameterList::cast)
    }
}

ast_node!(AstCircomProgram, CircomProgram);

impl AstCircomProgram {
    pub fn pragma(&self) -> Option<AstPragma> {
        self.syntax().children().find_map(AstPragma::cast)
    }

    pub fn template_list(&self) -> Vec<AstTemplateDef> {
        self.syntax()
            .children()
            .filter_map(AstTemplateDef::cast)
            .collect()
    }

    pub fn function_list(&self) -> Vec<AstFunctionDef> {
        self.syntax()
            .children()
            .filter_map(AstFunctionDef::cast)
            .collect()
    }

    pub fn get_template_by_name(
        &self,
        ast_template_name: &AstTemplateName,
    ) -> Option<AstTemplateDef> {
        for template in self.template_list() {
            if let Some(template_name) = template.template_name() {
                if template_name.same_name(ast_template_name) {
                    return Some(template);
                }
            }
        }
        None
    }
}

ast_node!(AstComponentCall, ComponentCall);

impl AstComponentCall {
    pub fn component_name(&self) -> Option<AstComponentIdentifier> {
        support::child(self.syntax())
    }
    pub fn signal(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstComponentIdentifier, ComponentIdentifier);

impl AstComponentIdentifier {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
