//! Lexical, scope-aware symbol table.
//!
//! Phase B: a per-file index of declarations keyed by name within (a) the file top-level
//! (template + function names) and (b) each template/function body scope (params + direct-child
//! signal/var/component decls). Consumed by [`crate::resolver`] for sound, name-based resolution
//! that does not depend on token identity, which is what made the legacy `hash(text)` index
//! unsound for cross-file lookups (the model this replaces).

use std::collections::HashMap;

use lsp_types::Range;
use rowan::ast::AstNode;
use rowan::{TextRange, TextSize};
use syntax::syntax_node::SyntaxNode;

use syntax::abstract_syntax_tree::{
    AstCircomProgram, AstComponentDecl, AstIdentifier, AstInputSignalDecl, AstOutputSignalDecl,
    AstParameterList, AstSignalDecl, AstStatementList, AstVarDecl, Named,
};

use crate::file_db::FileDB;

/// The category of a declared symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Template,
    Function,
    Bus,
    Signal,
    Variable,
    Component,
    Param,
}

/// One declaration. One symbol == one definition [`Range`] (the former `Vec<Range>` merge was an
/// artifact of `hash(text)` collapsing same-named decls into one id).
#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    pub def_range: Range,
}

/// A template/function/bus body scope. `range` is the whole-unit **byte** range used for
/// cursor-containment: the scope whose `range` contains the token's start offset is the scope.
#[derive(Debug, Clone)]
struct Scope {
    range: TextRange,
    symbols: HashMap<String, Vec<Symbol>>,
}

/// The per-file declaration index: each template/function body in document order (`scopes`), plus
/// the file top-level (`top_level`) holding every template + function name. Built once from the
/// AST, then queried by name without needing the [`FileDB`].
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    top_level: HashMap<String, Vec<Symbol>>,
}

impl SymbolTable {
    /// Build the per-file index from `ast`, using `file_db` only to turn syntax ranges into LSP
    /// [`Range`]s. The returned table needs no [`FileDB`] at query time. A malformed AST (e.g. a
    /// nameless template) is skipped rather than crashing the single-threaded server.
    pub fn build(file_db: &FileDB, ast: &AstCircomProgram) -> SymbolTable {
        let mut table = SymbolTable::default();

        for template in ast.template_list() {
            let Some(name) = template.identifier() else {
                continue;
            };
            let name = name.syntax().text().to_string();
            table.index_unit(
                file_db,
                template.syntax(),
                &name,
                SymbolKind::Template,
                template.parameter_list(),
                template.statements(),
            );
        }

        for function in ast.function_list() {
            let Some(name) = function.identifier() else {
                continue;
            };
            let name = name.syntax().text().to_string();
            table.index_unit(
                file_db,
                function.syntax(),
                &name,
                SymbolKind::Function,
                function.parameter_list(),
                function.statements(),
            );
        }

        for bus in ast.bus_list() {
            let Some(name) = bus.identifier() else {
                continue;
            };
            let name = name.syntax().text().to_string();
            table.index_unit(
                file_db,
                bus.syntax(),
                &name,
                SymbolKind::Bus,
                bus.parameter_list(),
                bus.statements(),
            );
        }

        table
    }

    /// Index one top-level definition unit (template/function/bus) — the three share the
    /// `definition_body` grammar (`name (params)? block`), so their indexing is identical apart
    /// from `kind` and whether signal declarations are allowed (templates and buses have signals;
    /// functions do not). Centralizing it keeps the three from drifting.
    fn index_unit(
        &mut self,
        file_db: &FileDB,
        unit_syntax: &SyntaxNode,
        name: &str,
        kind: SymbolKind,
        params: Option<AstParameterList>,
        statements: Option<AstStatementList>,
    ) {
        let scope_range = unit_syntax.text_range();
        let def_range = file_db.range(unit_syntax);
        // Templates and buses carry signal declarations in their bodies; functions do not.
        let with_signals = kind != SymbolKind::Function;

        self.top_level
            .entry(name.to_string())
            .or_default()
            .push(Symbol {
                kind,
                name: name.to_string(),
                def_range,
            });

        let mut scope = Scope {
            range: scope_range,
            symbols: HashMap::new(),
        };
        index_params(&mut scope.symbols, file_db, params);
        if let Some(statements) = statements {
            if with_signals {
                index_signals(&mut scope.symbols, file_db, &statements);
            }
            index_vars(&mut scope.symbols, file_db, &statements);
            index_components(&mut scope.symbols, file_db, &statements);
        }
        self.scopes.push(scope);
    }

    /// Look up `name` in the file top-level scope (template + function names).
    pub fn lookup_top_level(&self, name: &str) -> &[Symbol] {
        self.top_level.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Look up `name` in the body scope whose byte range contains `offset`. The flat-scope model
    /// means a token sits in at most one scope (templates/functions do not nest).
    pub fn lookup_in_scope(&self, offset: TextSize, name: &str) -> &[Symbol] {
        self.scopes
            .iter()
            .find(|s| s.range.contains(offset))
            .and_then(|s| s.symbols.get(name))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// Index one named declaration (`signal`/`var`/`component`) into `symbols` keyed by its
/// [`Named::identifier`] text, using the whole declaration node's range as the definition range.
///
/// Tuple-form declarations (`var (a,b) = …`, `signal (a,b) <== …`) have no wrapping
/// `ComplexIdentifier` — [`Named::identifier`] returns `None` — so their names are bare
/// `Identifier` children of the declaration node; each is indexed individually (else goto-def on a
/// tuple-declared name would resolve to nothing).
fn index_decl<N: Named>(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    kind: SymbolKind,
    decl: &N,
) {
    if let Some(id) = decl.identifier() {
        let name = id.syntax().text().to_string();
        symbols.entry(name.clone()).or_default().push(Symbol {
            kind,
            name,
            def_range: file_db.range(decl.syntax()),
        });
        return;
    }
    // Tuple form: the names are bare `Identifier` children (no `ComplexIdentifier` wrapper).
    for id in decl.syntax().children().filter_map(AstIdentifier::cast) {
        let name = id.syntax().text().to_string();
        symbols.entry(name.clone()).or_default().push(Symbol {
            kind,
            name,
            def_range: file_db.range(id.syntax()),
        });
    }
}

/// Index every parameter of `params` (function/template args) as a [`SymbolKind::Param`].
fn index_params(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    params: Option<AstParameterList>,
) {
    let Some(params) = params else {
        return;
    };
    for param in params.parameters() {
        let name = param.syntax().text().to_string();
        symbols.entry(name.clone()).or_default().push(Symbol {
            kind: SymbolKind::Param,
            name,
            def_range: file_db.range(param.syntax()),
        });
    }
}

/// Index input/output/intermediate signal declarations (templates only) as
/// [`SymbolKind::Signal`].
fn index_signals(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    statements: &AstStatementList,
) {
    for signal in statements.find_children::<AstInputSignalDecl>() {
        index_decl(symbols, file_db, SymbolKind::Signal, &signal);
    }
    for signal in statements.find_children::<AstOutputSignalDecl>() {
        index_decl(symbols, file_db, SymbolKind::Signal, &signal);
    }
    for signal in statements.find_children::<AstSignalDecl>() {
        index_decl(symbols, file_db, SymbolKind::Signal, &signal);
    }
}

/// Index `var` declarations as [`SymbolKind::Variable`].
fn index_vars(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    statements: &AstStatementList,
) {
    for var in statements.find_children::<AstVarDecl>() {
        index_decl(symbols, file_db, SymbolKind::Variable, &var);
    }
}

/// Index `component` declarations as [`SymbolKind::Component`].
fn index_components(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    statements: &AstStatementList,
) {
    for component in statements.find_children::<AstComponentDecl>() {
        index_decl(symbols, file_db, SymbolKind::Component, &component);
    }
}
