use parser::{Rule, Token};
use rowan::ast::{support, AstNode};

use super::*;
use crate::syntax_node::SyntaxKind;

// ============ Program ============

impl Program {
    pub fn pragma(&self) -> Option<Pragma> {
        support::child(self.syntax())
    }

    pub fn includes(&self) -> impl Iterator<Item = Include> {
        support::children(self.syntax())
    }

    pub fn templates(&self) -> impl Iterator<Item = Template> {
        support::children(self.syntax())
    }

    pub fn functions(&self) -> impl Iterator<Item = Function> {
        support::children(self.syntax())
    }

    pub fn find_template(&self, name: &str) -> Option<Template> {
        self.templates()
            .find(|t| t.name().map(|n| n.text() == name).unwrap_or(false))
    }

    pub fn find_function(&self, name: &str) -> Option<Function> {
        self.functions()
            .find(|f| f.name().map(|n| n.text() == name).unwrap_or(false))
    }
}

// ============ Template ============

impl Template {
    pub fn name(&self) -> Option<Ident> {
        token_child(self.syntax())
    }

    pub fn params(&self) -> Option<ParamList> {
        support::child(self.syntax())
    }

    pub fn body(&self) -> Option<Block> {
        support::child(self.syntax())
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.body()
            .into_iter()
            .flat_map(|b| support::children(b.syntax()))
    }

    pub fn signals(&self) -> impl Iterator<Item = SignalDecl> {
        self.body()
            .into_iter()
            .flat_map(|b| support::children(b.syntax()))
    }

    pub fn vars(&self) -> impl Iterator<Item = VarDecl> {
        self.body()
            .into_iter()
            .flat_map(|b| support::children(b.syntax()))
    }

    pub fn components(&self) -> impl Iterator<Item = ComponentDecl> {
        self.body()
            .into_iter()
            .flat_map(|b| support::children(b.syntax()))
    }
}

// ============ Function ============

impl Function {
    pub fn name(&self) -> Option<Ident> {
        token_child(self.syntax())
    }

    pub fn params(&self) -> Option<ParamList> {
        support::child(self.syntax())
    }

    pub fn body(&self) -> Option<Block> {
        support::child(self.syntax())
    }

    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.body()
            .into_iter()
            .flat_map(|b| support::children(b.syntax()))
    }

    pub fn vars(&self) -> impl Iterator<Item = VarDecl> {
        self.body()
            .into_iter()
            .flat_map(|b| support::children(b.syntax()))
    }

    pub fn components(&self) -> impl Iterator<Item = ComponentDecl> {
        self.body()
            .into_iter()
            .flat_map(|b| support::children(b.syntax()))
    }
}

// ============ SignalDecl ============

impl SignalDecl {
    pub fn is_input(&self) -> bool {
        self.signal_io().map_or(false, |io| io.is_input())
    }

    pub fn is_output(&self) -> bool {
        self.signal_io().map_or(false, |io| io.is_output())
    }

    pub fn is_internal(&self) -> bool {
        self.signal_io().is_none()
    }

    pub fn signal_io(&self) -> Option<SignalIo> {
        support::child(self.syntax())
    }

    pub fn name(&self) -> Option<Ident> {
        self.ident()
    }

    pub fn ident(&self) -> Option<Ident> {
        self.syntax()
            .children()
            .find(|child| child.kind() == SyntaxKind::from_rule(Rule::SignalInitList))
            .and_then(|list| {
                list.children()
                    .find(|child| child.kind() == SyntaxKind::from_rule(Rule::SignalInit))
            })
            .and_then(|init| {
                init.children()
                    .find(|child| child.kind() == SyntaxKind::from_rule(Rule::ComplexId))
            })
            .and_then(|complex_id| token_child(&complex_id))
    }
}

impl SignalIo {
    pub fn is_input(&self) -> bool {
        self.has_token(Token::Input)
    }

    pub fn is_output(&self) -> bool {
        self.has_token(Token::Output)
    }

    fn has_token(&self, token: Token) -> bool {
        self.syntax().children_with_tokens().any(|c| {
            c.as_token()
                .map_or(false, |t| t.kind() == SyntaxKind::from_token(token))
        })
    }
}

// ============ VarDecl ============

impl VarDecl {
    pub fn name(&self) -> Option<Ident> {
        self.ident()
    }

    pub fn ident(&self) -> Option<Ident> {
        // Look for ComplexId at any depth within the VarDecl
        // Structure: VarDecl -> VarInitList -> VarInit -> ComplexId -> Ident
        for child in self.syntax().children() {
            for grandchild in child.children() {
                if ComplexId::can_cast(grandchild.kind()) {
                    if let Some(complex_id) = ComplexId::cast(grandchild) {
                        if let Some(ident) = token_child(complex_id.syntax()) {
                            return Some(ident);
                        }
                    }
                } else {
                    for greatgrand in grandchild.children() {
                        if ComplexId::can_cast(greatgrand.kind()) {
                            if let Some(complex_id) = ComplexId::cast(greatgrand) {
                                if let Some(ident) = token_child(complex_id.syntax()) {
                                    return Some(ident);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

// ============ ComponentDecl ============

impl ComponentDecl {
    pub fn name(&self) -> Option<Ident> {
        self.ident()
    }

    pub fn ident(&self) -> Option<Ident> {
        support::child::<ComplexId>(self.syntax()).and_then(|c| token_child(c.syntax()))
    }

    pub fn template_call(&self) -> Option<TemplateCall> {
        support::child(self.syntax())
    }
}

// ============ TemplateCall ============

impl TemplateCall {
    pub fn template_name(&self) -> Option<Ident> {
        token_child(self.syntax())
    }

    pub fn args(&self) -> Option<ArgList> {
        support::child(self.syntax())
    }
}

// ============ Include ============

impl Include {
    pub fn path(&self) -> Option<std::string::String> {
        token_child::<StringLiteral>(self.syntax()).map(|s| s.value())
    }
}

// ============ Pragma ============

impl Pragma {
    pub fn version(&self) -> Option<Version> {
        token_child(self.syntax())
    }
}

// ============ ParamList ============

impl ParamList {
    pub fn idents(&self) -> impl Iterator<Item = Ident> {
        self.syntax().children().flat_map(|child| {
            child
                .children_with_tokens()
                .filter_map(|elem| elem.into_token().and_then(Ident::cast))
        })
    }
}

// ============ Block ============

impl Block {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        support::children(self.syntax())
    }
}

// ============ ComplexId ============

impl ComplexId {
    pub fn name(&self) -> Option<Ident> {
        token_child(self.syntax())
    }
}

// ============ StringLiteral ============

impl StringLiteral {
    pub fn value(&self) -> std::string::String {
        let text = self.text();
        let len = text.len();
        if len >= 2 {
            text[1..len - 1].to_string()
        } else {
            std::string::String::new()
        }
    }
}
