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
///
/// `def_range` is the **identifier** (the name token) — used by goto-definition/rename/references
/// so they jump to the name, not the start of the statement. `decl_range` is the **whole
/// declaration** (statement/unit) — used by hover to show the full declaration text.
///
/// `type_name` is set only for [`SymbolKind::Component`] — the name of the template it instantiates
/// (`component c = T();` → `Some("T")`); it's the component's "type" used by member completion.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    pub def_range: Range,
    pub decl_range: Range,
    pub type_name: Option<String>,
}

/// A template/function/bus body scope. `range` is the whole-unit **byte** range used for
/// cursor-containment; `name`/`kind` identify which unit the scope belongs to so a template's signal
/// members can be looked up by name (member completion: a component's members are its template's
/// signals).
#[derive(Debug, Clone)]
struct Scope {
    name: String,
    kind: SymbolKind,
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

    /// Index one top-level definition unit (template/function/bus) — the three share the
    /// `definition_body` grammar (`name (params)? block`), so their indexing is identical apart
    /// from `kind` and whether signal declarations are allowed (templates and buses have signals;
    /// functions do not). Centralizing it keeps the three from drifting.
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

    /// The single body scope whose byte range contains `offset`, or `None`. The flat-scope model
    /// means a token sits in at most one scope (templates/functions do not nest). Shared by
    /// [`lookup_in_scope`] (rename/references via `resolver::resolve`) and [`scope_symbols_at`]
    /// (completion) so the two features can never disagree on which scope an offset belongs to.
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

    /// Every symbol declared in the body scope containing `offset` (params/signals/vars/
    /// components), or empty if the offset is outside any body — completion's in-scope names.
    pub fn scope_symbols_at(&self, offset: TextSize) -> Vec<&Symbol> {
        self.scope_at(offset)
            .map(|s| s.symbols.values().flatten().collect::<Vec<_>>())
            .unwrap_or_default()
    }

    /// The instantiated template name of the component `name` in the scope at `offset`, or `None`
    /// (member completion: the receiver's type). Used to look up a component's members.
    pub fn component_type_at(&self, offset: TextSize, name: &str) -> Option<&str> {
        let scope = self.scope_at(offset)?;
        scope
            .symbols
            .get(name)?
            .iter()
            .find(|s| s.kind == SymbolKind::Component)
            .and_then(|s| s.type_name.as_deref())
    }

    /// The signal members of the template named `template_name` — its declared input/output/
    /// intermediate signals. Empty if `template_name` isn't a template in this file (a component's
    /// members are exactly its template's signals; params/vars/components of the template are not
    /// members).
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

/// Index input/output/intermediate signal declarations (templates only) as
/// [`SymbolKind::Signal`]. Uses `descendants` (not `find_children`) so signals inside nested blocks
/// (`for`/`if`/`while` bodies) are also indexed.
fn index_signals(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    statements: &AstStatementList,
) {
    for signal in statements
        .syntax()
        .descendants()
        .filter_map(AstInputSignalDecl::cast)
    {
        index_decl(symbols, file_db, SymbolKind::Signal, &signal);
    }
    for signal in statements
        .syntax()
        .descendants()
        .filter_map(AstOutputSignalDecl::cast)
    {
        index_decl(symbols, file_db, SymbolKind::Signal, &signal);
    }
    for signal in statements
        .syntax()
        .descendants()
        .filter_map(AstSignalDecl::cast)
    {
        index_decl(symbols, file_db, SymbolKind::Signal, &signal);
    }
}

/// Index `var` declarations as [`SymbolKind::Variable`]. Uses `descendants` so loop variables
/// (`for (var i = …)`) and vars inside nested blocks are also indexed.
fn index_vars(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    statements: &AstStatementList,
) {
    for var in statements
        .syntax()
        .descendants()
        .filter_map(AstVarDecl::cast)
    {
        index_decl(symbols, file_db, SymbolKind::Variable, &var);
    }
}

/// Index `component` declarations as [`SymbolKind::Component`], recording the instantiated template
/// name (`component c = T();` → `type_name = Some("T")`) for member completion. Components have no
/// tuple form in circom, so the symbol is built directly (not via `index_decl`). Uses `descendants`
/// so components inside nested blocks are also indexed.
fn index_components(
    symbols: &mut HashMap<String, Vec<Symbol>>,
    file_db: &FileDB,
    statements: &AstStatementList,
) {
    for component in statements
        .syntax()
        .descendants()
        .filter_map(AstComponentDecl::cast)
    {
        let Some(id) = component.identifier() else {
            continue;
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
}
