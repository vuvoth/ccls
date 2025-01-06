use parser::token_kind::TokenKind::*;
use rowan::ast::AstChildren;
use rowan::SyntaxText;

use crate::syntax_node::CircomLanguage;
use crate::syntax_node::SyntaxNode;
use parser::token_kind::TokenKind;
use rowan::ast::{support, AstNode};

use super::template::AstTemplateDef;
use super::template::AstTemplateName;

ast_node!(AstSignalHeader, SignalHeader);
ast_node!(AstInputSignalDecl, InputSignalDecl);
ast_node!(AstOutputSignalDecl, OutputSignalDecl);
ast_node!(AstSignalDecl, SignalDecl);

impl AstInputSignalDecl {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }

    pub fn same_name(&self, other: &SyntaxText) -> bool {
        if let Some(name) = self.name() {
            return name.equal(other);
        }
        false
    }
}

impl AstOutputSignalDecl {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
impl AstSignalDecl {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}
ast_node!(AstVarDecl, VarDecl);

impl AstVarDecl {
    pub fn name(&self) -> Option<AstIdentifier> {
        support::child(self.syntax())
    }
}

ast_node!(AstComponentDecl, ComponentDecl);

// component hash = Poseidon(2);
// template --> Poseidon
// component_identifier --> hash
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

    pub fn find_children<N: AstNode<Language = CircomLanguage>>(&self) -> Vec<N> {
        self.syntax().children().filter_map(N::cast).collect()
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

impl AstParameterList {
    pub fn parameters(&self) -> Vec<AstIdentifier> {
        self.syntax()
            .children()
            .filter_map(AstIdentifier::cast)
            .collect()
    }
}

ast_node!(AstIdentifier, Identifier);

impl AstIdentifier {
    pub fn equal(&self, other: &SyntaxText) -> bool {
        self.syntax().text() == *other
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
    pub fn libs(&self) -> Vec<AstInclude> {
        self.syntax()
            .children()
            .filter_map(AstInclude::cast)
            .collect()
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
            if let Some(template_name) = template.name() {
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

ast_node!(AstCircomString, CircomString);
impl AstCircomString {
    pub fn value(&self) -> String {
        let text = &self.syntax().text().to_string();
        text[1..text.len() - 1].to_string()
    }
}

ast_node!(AstInclude, IncludeKw);

impl AstInclude {
    pub fn lib(&self) -> Option<AstCircomString> {
        support::child(self.syntax())
    }
}
