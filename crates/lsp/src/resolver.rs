//! Pure token → definition resolver over a [`SymbolTable`].
//!
//! Given a token under the cursor and a per-file [`SymbolTable`], returns the symbol(s) the token
//! refers to as [`ResolvedSymbol`]s (kind + name + definition range). This is the single resolution
//! core shared by `goto_definition` today and by `hover`/`references`/`rename` later. It owns no IO
//! and no LSP request types — the caller maps each [`ResolvedSymbol::def_range`] into a
//! file-scoped [`lsp_types::Location`].
//!
//! Resolution is purely name-based against the table, so it is sound across files: the same name
//! resolves in each file's own table without reusing a foreign token's identity (the bug that made
//! the legacy `hash(text)` index structurally return `None` for cross-file signal/var/param lookups).

use lsp_types::Range;
use rowan::TextSize;

use syntax::syntax_node::SyntaxToken;

use crate::semantic::SymbolTable;

pub use crate::semantic::SymbolKind;

/// A resolved reference: what the token means, and where it is defined. A single declaration
/// per symbol (the former `Vec<Range>` was an artifact of `hash(text)` collapsing same-named decls
/// into one id). It is URL-agnostic — the caller tags each range with its owning file when shaping
/// [`lsp_types::Location`]s.
#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    pub kind: SymbolKind,
    pub name: String,
    pub def_range: Range,
}

/// Resolve `token` against `table`: look up `token.text()` in the body scope containing the token's
/// start offset, and also in the file top-level (so a template name used in `component mul = T()`
/// and a scope's own name token both resolve). Returns one [`ResolvedSymbol`] per matching `Symbol`.
///
/// The body and file name spaces are disjoint (bodies carry params/signals/vars/components; the
/// file carries template/function names), so the two lookups never duplicate the same declaration.
/// Only `Identifier` tokens are resolved here — the include-path `CircomString` is routed to
/// `jump_to_lib` by the handler, never the resolver.
pub fn resolve(table: &SymbolTable, token: &SyntaxToken) -> Vec<ResolvedSymbol> {
    let name = token.text();
    let offset: TextSize = token.text_range().start();

    let scope_symbols = table.lookup_in_scope(offset, name);
    let top_level_symbols = table.lookup_top_level(name);

    let mut resolved = Vec::with_capacity(scope_symbols.len() + top_level_symbols.len());
    for sym in scope_symbols.iter().chain(top_level_symbols.iter()) {
        resolved.push(ResolvedSymbol {
            kind: sym.kind,
            name: sym.name.clone(),
            def_range: sym.def_range,
        });
    }
    resolved
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::Url;
    use parser::token_kind::TokenKind;
    use rowan::ast::AstNode;
    use syntax::abstract_syntax_tree::{
        AstCircomProgram, AstComponentCall, AstInputSignalDecl, AstVarDecl,
    };
    use syntax::syntax::syntax_tree;
    use syntax::syntax_node::{CircomLanguage, SyntaxToken};

    use crate::file_db::{FileDB, FileId};
    use crate::semantic::SymbolTable;

    use super::{resolve, SymbolKind};

    /// Build the (file_db, ast, symbol_table) triple the resolver runs against, from inline source.
    /// Drops the legacy `SemanticDB` round-trip entirely — the table is built directly from the AST.
    fn index(source: &str) -> (FileDB, AstCircomProgram, SymbolTable) {
        let file = FileDB::new(
            FileId(0),
            source,
            Url::from_file_path(Path::new("/tmp/test.circom")).unwrap(),
        );
        let node = syntax_tree(source);
        let ast = AstCircomProgram::cast(node).expect("source should parse to a program");
        let table = SymbolTable::build(&file, &ast);
        (file, ast, table)
    }

    /// All `Identifier` leaf tokens in `ast` whose text equals `text`, in document order.
    fn tokens_with_text(ast: &AstCircomProgram, text: &str) -> Vec<SyntaxToken> {
        ast.syntax()
            .descendants_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == TokenKind::Identifier && t.text() == text)
            .collect()
    }

    /// The first token named `text` that is **not** wrapped in a declaration node of type `N` —
    /// i.e. a usage rather than its declaration. Falls back to the first token if none qualifies.
    fn usage_token<N: AstNode<Language = CircomLanguage>>(
        ast: &AstCircomProgram,
        text: &str,
    ) -> SyntaxToken {
        tokens_with_text(ast, text)
            .into_iter()
            .find(|t| {
                t.parent()
                    .and_then(|p| p.ancestors().find_map(N::cast))
                    .is_none()
            })
            .unwrap_or_else(|| tokens_with_text(ast, text).remove(0))
    }

    /// A signal **usage** resolves to its declaration range. `a` in `c <== a + 0;` must jump back
    /// to the `signal input a;` line, not to itself.
    #[test]
    fn signal_usage_resolves_to_decl_test() {
        let source = "pragma circom 2.0.0;
template T() {
    signal input a;
    signal output c;
    c <== a + 0;
}
";
        let (_file, ast, table) = index(source);
        let token = usage_token::<AstInputSignalDecl>(&ast, "a");

        let resolved = resolve(&table, &token);

        assert!(
            resolved.iter().any(|s| s.kind == SymbolKind::Signal),
            "expected a Signal resolution, got {resolved:?}"
        );
        let signal = resolved
            .iter()
            .find(|s| s.kind == SymbolKind::Signal)
            .unwrap();
        // The definition range is the whole `signal input a;` statement, which starts on line 3.
        assert_eq!(
            signal.def_range.start.line, 2,
            "decl is on line 3 (0-indexed 2)"
        );
        assert_eq!(signal.name, "a");
    }

    /// A template **name** (its own definition identifier) resolves to the template def range.
    #[test]
    fn template_name_resolves_to_def_test() {
        let source = "pragma circom 2.0.0;
template Multiplier2() {
    signal input a;
    signal input b;
    signal output c;
    c <== a * b;
}
";
        let (_file, ast, table) = index(source);
        // The template's own name token matches the file-scope Template entry.
        let token = tokens_with_text(&ast, "Multiplier2").remove(0);

        let resolved = resolve(&table, &token);

        assert!(
            resolved
                .iter()
                .any(|s| s.kind == SymbolKind::Template && s.name == "Multiplier2"),
            "expected a Template resolution, got {resolved:?}"
        );
        let template = resolved
            .iter()
            .find(|s| s.kind == SymbolKind::Template)
            .unwrap();
        // The def range spans the whole `template Multiplier2() { ... }` block, starting on line 2.
        assert_eq!(template.def_range.start.line, 1);
    }

    /// A variable **usage** inside a function resolves to its `var` declaration range.
    #[test]
    fn variable_usage_resolves_to_decl_test() {
        let source = "pragma circom 2.0.0;
function hash(x) {
    var y = x;
    return y;
}
";
        let (_file, ast, table) = index(source);
        let token = usage_token::<AstVarDecl>(&ast, "y");

        let resolved = resolve(&table, &token);

        assert!(
            resolved.iter().any(|s| s.kind == SymbolKind::Variable),
            "expected a Variable resolution, got {resolved:?}"
        );
        let var = resolved
            .iter()
            .find(|s| s.kind == SymbolKind::Variable)
            .unwrap();
        // `var y = x;` lives on line 3 (0-indexed 2).
        assert_eq!(var.def_range.start.line, 2);
        assert_eq!(var.name, "y");
    }

    /// A function **name** resolves to the function def range.
    #[test]
    fn function_name_resolves_to_def_test() {
        let source = "pragma circom 2.0.0;
function hash(x) {
    var y = x;
    return y;
}
";
        let (_file, ast, table) = index(source);
        let token = tokens_with_text(&ast, "hash").remove(0);

        let resolved = resolve(&table, &token);

        assert!(
            resolved
                .iter()
                .any(|s| s.kind == SymbolKind::Function && s.name == "hash"),
            "expected a Function resolution, got {resolved:?}"
        );
        let function = resolved
            .iter()
            .find(|s| s.kind == SymbolKind::Function)
            .unwrap();
        // `function hash(x) { ... }` starts on line 2 (0-indexed 1).
        assert_eq!(function.def_range.start.line, 1);
    }

    /// A signal accessed through a component (`mul.in1`) is a member-access — out of scope for the
    /// flat name resolver by design. `in1` here sits in `Main`'s body, which has no `in1` (it lives
    /// in `Multiplier2`'s scope), and the file scope only carries template names, so resolution
    /// returns empty. Pins the intended no-op (not a panic) for component-call fields.
    #[test]
    fn component_call_field_returns_empty_test() {
        let source = "pragma circom 2.0.0;
template Multiplier2() {
    signal input in1;
    signal input in2;
    signal output out;
    out <== in1 * in2;
}
template Main() {
    component mul = Multiplier2();
    mul.in1 <== 1;
}
";
        let (_file, ast, table) = index(source);
        // Pick the `in1` wrapped by a `ComponentCall` node (`mul.in1`), not the free-standing one.
        let token = tokens_with_text(&ast, "in1")
            .into_iter()
            .find(|t| {
                t.parent()
                    .and_then(|p| p.ancestors().find_map(AstComponentCall::cast))
                    .is_some()
            })
            .expect("a `mul.in1` ComponentCall token should exist");

        let resolved = resolve(&table, &token);
        assert!(
            resolved.is_empty(),
            "component-call field is a no-op by design, got {resolved:?}"
        );
    }
}
