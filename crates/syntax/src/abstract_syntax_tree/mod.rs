/// Declare a typed AST node wrapping a rowan `SyntaxNode` of kind `$kind`.
///
/// Generates a newtype struct, the `AstNode` impl (`can_cast`/`cast`/`syntax`), and an inherent
/// [`text`](#) accessor, so every node uniformly supports `.syntax()` and `.text()` with no
/// per-node boilerplate. All other accessors are written as plain `impl` blocks using
/// `support::child`/`support::children` for a single, consistent traversal style.
#[macro_export]
macro_rules! ast_node {
    ($ast_name: ident, $kind: expr) => {
        #[derive(Debug, Clone)]
        pub struct $ast_name {
            syntax: SyntaxNode,
        }

        impl $ast_name {
            /// The source text this node spans.
            pub fn text(&self) -> rowan::SyntaxText {
                self.syntax.text()
            }
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
                if Self::can_cast(syntax.kind()) {
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

pub mod ast;
pub mod expression;
pub mod statement;
pub mod template;

pub use ast::*;
pub use expression::*;
pub use statement::*;
pub use template::*;
