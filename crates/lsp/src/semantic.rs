//! Lexical, scope-aware symbol table — a per-file index of declarations by name: the file
//! top-level (template/function/bus names) and each body scope (params/signals/vars/components).
//! Consumed by [`crate::resolver`] for name-based resolution that doesn't depend on token identity
//! (which made the legacy `hash(text)` index unsound cross-file).

use std::collections::HashMap;

use lsp_types::Range;
use parser::token_kind::TokenKind;
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

/// One declaration == one definition [`Range`] (the legacy `Vec<Range>` merge was a `hash(text)`
/// artifact). `def_range` is the **name token** (goto-def/rename/references jump to the name);
/// `decl_range` is the **whole declaration** (hover shows full text). `type_name` is the
/// instantiated template for [`SymbolKind::Component`] only (`component c = T();` → `Some("T")`).
#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    pub def_range: Range,
    pub decl_range: Range,
    pub type_name: Option<String>,
}

/// A template/function/bus body scope. `range` is the whole-unit **byte** range for
/// cursor-containment; `name`/`kind` identify the unit so a template's signal members can be looked
/// up by name (a component's members are its template's signals).
#[derive(Debug, Clone)]
struct Scope {
    name: String,
    kind: SymbolKind,
    range: TextRange,
    symbols: HashMap<String, Vec<Symbol>>,
}

/// The per-file declaration index: body scopes in document order (`scopes`) + the file top-level
/// (`top_level`, template/function/bus names). Built once from the AST, queried by name without the
/// [`FileDB`].
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    scopes: Vec<Scope>,
    top_level: HashMap<String, Vec<Symbol>>,
}

impl SymbolTable {
    /// Build the per-file index from `ast` (`file_db` only maps syntax ranges to LSP [`Range`]s).
    /// A malformed AST (e.g. a nameless template) is skipped, not crashed.
    pub fn build(file_db: &FileDB, ast: &AstCircomProgram) -> SymbolTable {
        let mut table = SymbolTable::default();

        for template in ast.template_list() {
            let Some(name) = template.identifier() else {
                continue;
            };
            let name_text = name.syntax().text().to_string();
            table.index_unit(
                file_db,
                template.syntax(),
                name.syntax(),
                &name_text,
                SymbolKind::Template,
                template.parameter_list(),
                template.statements(),
            );
        }

        for function in ast.function_list() {
            let Some(name) = function.identifier() else {
                continue;
            };
            let name_text = name.syntax().text().to_string();
            table.index_unit(
                file_db,
                function.syntax(),
                name.syntax(),
                &name_text,
                SymbolKind::Function,
                function.parameter_list(),
                function.statements(),
            );
        }

        for bus in ast.bus_list() {
            let Some(name) = bus.identifier() else {
                continue;
            };
            let name_text = name.syntax().text().to_string();
            table.index_unit(
                file_db,
                bus.syntax(),
                name.syntax(),
                &name_text,
                SymbolKind::Bus,
                bus.parameter_list(),
                bus.statements(),
            );
        }

        table
    }

    /// Index one top-level unit (template/function/bus) — they share the `definition_body` grammar
    /// (`name (params)? block`), differing only in `kind` and whether signals are allowed (functions
    /// have none). Centralized to keep the three from drifting.
    #[allow(clippy::too_many_arguments)]
    fn index_unit(
        &mut self,
        file_db: &FileDB,
        unit_syntax: &SyntaxNode,
        name_syntax: &SyntaxNode,
        name: &str,
        kind: SymbolKind,
        params: Option<AstParameterList>,
        statements: Option<AstStatementList>,
    ) {
        let scope_range = unit_syntax.text_range();
        let decl_range = file_db.range(unit_syntax);
        let def_range = file_db.range(name_syntax);
        // Templates and buses carry signal declarations in their bodies; functions do not.
        let with_signals = kind != SymbolKind::Function;

        self.top_level
            .entry(name.to_string())
            .or_default()
            .push(Symbol {
                kind,
                name: name.to_string(),
                def_range,
                decl_range,
                type_name: None,
            });

        let mut scope = Scope {
            name: name.to_string(),
            kind,
            range: scope_range,
            symbols: HashMap::new(),
        };
        index_params(&mut scope.symbols, file_db, params);
        if let Some(statements) = statements {
            index_body_symbols(&mut scope.symbols, file_db, &statements, with_signals);
        }
        self.scopes.push(scope);
    }

    /// Look up `name` in the file top-level scope (template + function names).
    pub fn lookup_top_level(&self, name: &str) -> &[Symbol] {
        self.top_level.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The single body scope whose byte range contains `offset`, or `None` (flat-scope model: a
    /// token is in at most one scope). Shared by [`lookup_in_scope`] and [`scope_symbols_at`] so
    /// rename/references and completion never disagree on scope membership.
    fn scope_at(&self, offset: TextSize) -> Option<&Scope> {
        self.scopes.iter().find(|s| s.range.contains(offset))
    }

    /// Look up `name` in the body scope containing `offset`.
    pub fn lookup_in_scope(&self, offset: TextSize, name: &str) -> &[Symbol] {
        self.scope_at(offset)
            .and_then(|s| s.symbols.get(name))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every top-level symbol (template/function/bus) in the file — completion's global names.
    pub fn top_level_symbols(&self) -> Vec<&Symbol> {
        self.top_level.values().flatten().collect()
    }

    /// Every symbol in the body scope containing `offset` (params/signals/vars/components), or
    /// empty — completion's in-scope names.
    pub fn scope_symbols_at(&self, offset: TextSize) -> Vec<&Symbol> {
        self.scope_at(offset)
            .map(|s| s.symbols.values().flatten().collect::<Vec<_>>())
            .unwrap_or_default()
    }

    /// The instantiated template name of component `name` at `offset`, or `None` (member
    /// completion: the receiver's type).
    pub fn component_type_at(&self, offset: TextSize, name: &str) -> Option<&str> {
        let scope = self.scope_at(offset)?;
        scope
            .symbols
            .get(name)?
            .iter()
            .find(|s| s.kind == SymbolKind::Component)
            .and_then(|s| s.type_name.as_deref())
    }

    /// The signal members of template `template_name` (its input/output/intermediate signals), or
    /// empty if not a template here. A component's members are exactly its template's signals —
    /// params/vars/components are not members.
    pub fn members_of(&self, template_name: &str) -> Vec<&Symbol> {
        self.scopes
            .iter()
            .find(|s| s.name == template_name && s.kind == SymbolKind::Template)
            .into_iter()
            .flat_map(|s| s.symbols.values().flatten())
            .filter(|s| s.kind == SymbolKind::Signal)
            .collect()
    }
}

/// Index one named declaration (`signal`/`var`/`component`) into `symbols` by name. Tuple-form
/// decls (`var (a,b) = …`) have no wrapping `ComplexIdentifier` ([`Named::identifier`] ⇒ `None`),
/// so their bare `Identifier` children are indexed each (else goto-def on a tuple name resolves to
/// nothing).
fn index_decl<N: Named>(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    kind: SymbolKind,
    decl: &N,
) {
    let decl_range = file_db.range(decl.syntax());
    if let Some(id) = decl.identifier() {
        let name = id.syntax().text().to_string();
        symbols.entry(name.clone()).or_default().push(Symbol {
            kind,
            name,
            def_range: file_db.range(id.syntax()),
            decl_range,
            type_name: None,
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
            decl_range,
            type_name: None,
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
        let range = file_db.range(param.syntax());
        symbols.entry(name.clone()).or_default().push(Symbol {
            kind: SymbolKind::Param,
            name,
            def_range: range,
            decl_range: range,
            type_name: None,
        });
    }
}

/// Index every signal/var/component declaration in `statements` in one `descendants()` walk
/// (document order), dispatched on [`TokenKind`]. `with_signals` gates signal indexing (templates
/// and buses only; functions have none). Uses `descendants` so nested-block and `for`-loop decls
/// are indexed too.
fn index_body_symbols(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    statements: &AstStatementList,
    with_signals: bool,
) {
    for node in statements.syntax().descendants() {
        match node.kind() {
            TokenKind::InputSignalDecl if with_signals => {
                if let Some(decl) = AstInputSignalDecl::cast(node) {
                    index_decl(symbols, file_db, SymbolKind::Signal, &decl);
                }
            }
            TokenKind::OutputSignalDecl if with_signals => {
                if let Some(decl) = AstOutputSignalDecl::cast(node) {
                    index_decl(symbols, file_db, SymbolKind::Signal, &decl);
                }
            }
            TokenKind::SignalDecl if with_signals => {
                if let Some(decl) = AstSignalDecl::cast(node) {
                    index_decl(symbols, file_db, SymbolKind::Signal, &decl);
                }
            }
            TokenKind::VarDecl => {
                if let Some(decl) = AstVarDecl::cast(node) {
                    index_decl(symbols, file_db, SymbolKind::Variable, &decl);
                }
            }
            TokenKind::ComponentDecl => {
                if let Some(decl) = AstComponentDecl::cast(node) {
                    index_component(symbols, file_db, &decl);
                }
            }
            _ => {}
        }
    }
}

/// Index one `component` decl as [`SymbolKind::Component`], recording its instantiated template
/// (`component c = T();` → `type_name = Some("T")`) for member completion. Built directly (no tuple
/// form), not via `index_decl`.
fn index_component(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    component: &AstComponentDecl,
) {
    let Some(id) = component.identifier() else {
        return;
    };
    let name = id.syntax().text().to_string();
    // `component c = T();` — the template `T` is this component's "type".
    let type_name = component
        .template()
        .and_then(|t| t.name())
        .map(|n| n.syntax().text().to_string());
    symbols.entry(name.clone()).or_default().push(Symbol {
        kind: SymbolKind::Component,
        name,
        def_range: file_db.range(id.syntax()),
        decl_range: file_db.range(component.syntax()),
        type_name,
    });
}
