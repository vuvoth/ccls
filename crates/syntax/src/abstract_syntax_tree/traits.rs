use crate::syntax_node::CircomLanguage;
use rowan::ast::AstNode;

/// Trait for nodes that have a name
pub trait Named: AstNode<Language = CircomLanguage> {
    fn name(&self) -> Option<&str>;
}

/// Trait for nodes with a body
pub trait HasBody: AstNode<Language = CircomLanguage> {
    type Body: AstNode<Language = CircomLanguage>;
    fn body(&self) -> Option<Self::Body>;
}

/// Trait for nodes with parameters
pub trait HasParams: AstNode<Language = CircomLanguage> {
    fn params(&self) -> Option<super::ParamList>;
}

/// Trait for iterable children
pub trait HasChildren<T>: AstNode<Language = CircomLanguage> {
    fn children(&self) -> impl Iterator<Item = T>;
}
