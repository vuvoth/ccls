use crate::syntax_node::CircomLanguage;
pub use rowan::ast::{support, AstChildren, AstNode};

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

ast_node!(AstTemplateName, TemplateName);

ast_node!(AstTemplateDef, TemplateDef);

impl AstTemplateName {
    pub fn name(&self) -> Option<AstIdentifier> {
        self.syntax().children().find_map(AstIdentifier::cast)
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
}
