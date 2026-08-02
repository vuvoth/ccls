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

use syntax::abstract_syntax_tree::{
    AstCircomProgram, AstComponentDecl, AstFunctionDef, AstIdentifier, AstInputSignalDecl,
    AstOutputSignalDecl, AstParameterList, AstSignalDecl, AstStatementList, AstTemplateDef,
    AstVarDecl, Named,
};

use crate::file_db::FileDB;

/// The category of a declared symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Template,
    Function,
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

/// Whether a body scope belongs to a template or a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    Template,
    Function,
}

/// A template/function body scope. `range` is the whole-unit **byte** range used for
/// cursor-containment: the scope whose `range` contains the token's start offset is the scope.
#[derive(Debug, Clone)]
struct Scope {
    #[allow(dead_code)]
    kind: ScopeKind,
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
            table.add_template(file_db, &template, &name);
        }

        for function in ast.function_list() {
            let Some(name) = function.identifier() else {
                continue;
            };
            table.add_function(file_db, &function, &name);
        }

        table
    }

    fn add_template(&mut self, file_db: &FileDB, template: &AstTemplateDef, name: &AstIdentifier) {
        let name_str = name.syntax().text().to_string();
        let scope_range = template.syntax().text_range();
        let def_range = file_db.range(template.syntax());

        self.top_level
            .entry(name_str.clone())
            .or_default()
            .push(Symbol {
                kind: SymbolKind::Template,
                name: name_str.clone(),
                def_range,
            });

        let mut scope = Scope {
            kind: ScopeKind::Template,
            range: scope_range,
            symbols: HashMap::new(),
        };
        index_params(&mut scope.symbols, file_db, template.parameter_list());
        if let Some(statements) = template.statements() {
            index_signals(&mut scope.symbols, file_db, &statements);
            index_vars(&mut scope.symbols, file_db, &statements);
            index_components(&mut scope.symbols, file_db, &statements);
        }
        self.scopes.push(scope);
    }

    fn add_function(&mut self, file_db: &FileDB, function: &AstFunctionDef, name: &AstIdentifier) {
        let name_str = name.syntax().text().to_string();
        let scope_range = function.syntax().text_range();
        let def_range = file_db.range(function.syntax());

        self.top_level
            .entry(name_str.clone())
            .or_default()
            .push(Symbol {
                kind: SymbolKind::Function,
                name: name_str.clone(),
                def_range,
            });

        let mut scope = Scope {
            kind: ScopeKind::Function,
            range: scope_range,
            symbols: HashMap::new(),
        };
        index_params(&mut scope.symbols, file_db, function.parameter_list());
        if let Some(statements) = function.statements() {
            // Functions cannot declare signals, so only vars/components are indexed here.
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
fn index_decl<N: Named>(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    kind: SymbolKind,
    decl: &N,
) {
    let Some(id) = decl.identifier() else {
        return;
    };
    let name = id.syntax().text().to_string();
    symbols.entry(name.clone()).or_default().push(Symbol {
        kind,
        name,
        def_range: file_db.range(decl.syntax()),
    });
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
