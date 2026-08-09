use anyhow::Result;
use lsp_server::{Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders, DidCloseTextDocument,
    DidOpenTextDocument, Notification as _,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, GotoImplementation,
    HoverRequest, PrepareRenameRequest, References, Rename, Request as _,
};
use lsp_types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    FileChangeType, Location, Range, Url,
};
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use rowan::TextSize;
use syntax::abstract_syntax_tree::{
    AstCircomProgram, AstComponentCall, AstComponentDecl, AstMainComponent,
};
use syntax::node::SyntaxToken;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::file_db::{FileDB, FileId};
use crate::handler;
use crate::handler::goto_definition::include_target_location;
use crate::resolver::{self, token_ancestors, ResolvedSymbol};
use crate::source_db::{ContentCacheDb, SourceDatabase};
use crate::symbol_table::{Symbol, SymbolKind};

/// A didOpen/didChange notification normalized to its full text + URI.
#[derive(Debug)]
pub struct TextDocument {
    text: String,
    uri: Url,
}

impl From<DidOpenTextDocumentParams> for TextDocument {
    fn from(value: DidOpenTextDocumentParams) -> Self {
        Self {
            text: value.text_document.text,
            uri: value.text_document.uri,
        }
    }
}

impl From<DidChangeTextDocumentParams> for TextDocument {
    fn from(value: DidChangeTextDocumentParams) -> Self {
        // `didChange` may carry an empty `contentChanges`; index safely (a panic kills the
        // single-threaded server).
        let text = value
            .content_changes
            .into_iter()
            .next()
            .map(|c| c.text)
            .unwrap_or_default();
        Self {
            text,
            uri: value.text_document.uri,
        }
    }
}

/// Server-wide state shared by every handler. `source_db` caches text, parses, file DBs, and
/// per-file `SymbolTable`s (lazy-built, invalidated on edit); includes load from disk once then
/// cache.
pub struct GlobalState {
    pub source_db: ContentCacheDb,
    /// URIs the client has open (didOpen, not yet didClose). The watcher's Deleted/Changed arms
    /// skip these so an external disk touch can't clobber an open doc's unsaved buffer.
    pub(crate) open_documents: HashSet<Url>,
    /// Memoized [`Self::loaded_includes`] per origin; interior-mutable for the `&self` read path.
    /// Cleared by [`Self::drop_include_cache`] on any text/index mutation.
    loaded_includes_cache: RefCell<HashMap<FileId, Vec<FileId>>>,
    /// Cached `identifier text -> files containing it` index over `workspace_files`, used to prune
    /// workspace references/rename scans to files that actually mention the name. `None` = stale;
    /// rebuilt lazily, dropped on any mutation via [`Self::drop_include_cache`].
    identifier_index: RefCell<Option<HashMap<String, HashSet<FileId>>>>,
    /// Eagerly loaded workspace `.circom` files — the scan set for workspace occurrences/symbol.
    pub(crate) workspace_files: HashSet<FileId>,
}

/// A resolved cursor location (id, parse, file DB, byte offset) — the shared prologue of every
/// read-handler (`id_for_url → ast → file_db → offset`).
pub(crate) struct CursorContext {
    pub id: FileId,
    pub ast: AstCircomProgram,
    pub file_db: FileDB,
    pub offset: TextSize,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl GlobalState {
    /// Construct with workspace `roots`. Roots scope the project `.circom` walk that feeds the
    /// basename index; `include` resolution itself is circom-style (relative to the source file),
    /// not confined to the roots. Absolute include paths are rejected; relative/`..` includes
    /// resolve (and may reach files outside the roots, matching the circom compiler).
    pub fn new(roots: Vec<PathBuf>) -> Self {
        let mut source_db = ContentCacheDb::new();
        source_db.set_workspace_roots(roots);
        Self {
            source_db,
            open_documents: HashSet::new(),
            loaded_includes_cache: RefCell::new(HashMap::new()),
            identifier_index: RefCell::new(None),
            workspace_files: HashSet::new(),
        }
    }

    /// Drop the memoized [`Self::loaded_includes`] and the identifier index — call after any
    /// text/index mutation so stale data can't be served.
    pub(crate) fn drop_include_cache(&mut self) {
        self.loaded_includes_cache.borrow_mut().clear();
        *self.identifier_index.borrow_mut() = None;
    }

    /// Flush derived caches after a VFS mutation that may record a change-log entry (text
    /// delete/modify). Register-only mutations (no change recorded) call `drop_include_cache` only.
    fn after_vfs_mutation(&mut self) {
        self.source_db.invalidate_changed();
        self.drop_include_cache();
    }

    /// Resolve `(uri, position)` to a [`CursorContext`], or `None` if unknown, text-less, or
    /// unparseable. The no-text guard is load-bearing: the workspace walk interns paths with no text
    /// and a watcher `DELETED` can null an open doc's text — without it such an id would reach
    /// `file_text().expect()` and crash the single-threaded server.
    pub(crate) fn cursor_context(
        &self,
        uri: &Url,
        position: lsp_types::Position,
    ) -> Option<CursorContext> {
        let id = self.source_db.id_for_url(uri)?;
        self.source_db.vfs().file_text(id)?;
        let ast = self.source_db.ast(id)?;
        let file_db = self.source_db.file_db(id);
        let offset = file_db.offset(position);
        Some(CursorContext {
            id,
            ast,
            file_db,
            offset,
        })
    }

    /// LSP diagnostics for `uri` from its cached parse/lexer errors. Empty for unknown or
    /// textless files (never panics — mirrors the `cursor_context` None-text guard).
    pub fn diagnostics_for_uri(&self, uri: &Url) -> Vec<Diagnostic> {
        let Some(id) = self.source_db.id_for_url(uri) else {
            return Vec::new();
        };
        if self.source_db.vfs().file_text(id).is_none() {
            return Vec::new();
        }
        let file_db = self.source_db.file_db(id);
        let errors = self.source_db.errors(id);
        errors
            .iter()
            .map(|e| Diagnostic {
                range: Range {
                    start: file_db.position(e.range.start()),
                    end: file_db.position(e.range.end()),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("circom".to_string()),
                message: e.msg.clone(),
                ..Default::default()
            })
            .collect()
    }

    /// Dispatch an LSP request to its handler by method name; `Ok(None)` for unhandled methods.
    /// Add a request = one arm here + a handler module + a capability entry.
    pub fn handle_request(&self, req: Request) -> Result<Option<Response>> {
        let id = req.id.clone();
        match req.method.as_str() {
            GotoDefinition::METHOD => dispatch(self, id, req, handler::goto_definition::handle),
            GotoImplementation::METHOD => {
                dispatch(self, id, req, handler::goto_implementation::handle)
            }
            HoverRequest::METHOD => dispatch(self, id, req, handler::hover::handle),
            Completion::METHOD => dispatch(self, id, req, handler::completion::handle),
            References::METHOD => dispatch(self, id, req, handler::references::handle),
            DocumentSymbolRequest::METHOD => {
                dispatch(self, id, req, handler::document_symbol::handle)
            }
            Formatting::METHOD => dispatch(self, id, req, handler::formatting::handle),
            Rename::METHOD => dispatch(self, id, req, handler::rename::handle),
            PrepareRenameRequest::METHOD => dispatch(self, id, req, handler::rename::prepare),
            "workspace/symbol" => dispatch(self, id, req, handler::workspace_symbol::handle),
            _ => Ok(None),
        }
    }

    /// Dispatch an LSP notification. Document open/change load includes and publish diagnostics;
    /// didClose drops open-doc tracking and clears diagnostics; watched-file changes keep the
    /// project basename index fresh (created/deleted `.circom` files); workspace-folder changes
    /// walk the newly-added roots. Returns `(uri, diagnostics)` pairs for the main loop to publish.
    pub fn handle_notification(
        &mut self,
        not: Notification,
    ) -> Result<Vec<(Url, Vec<Diagnostic>)>> {
        let mut to_publish = Vec::new();
        match not.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                let uri = params.text_document.uri.clone();
                self.handle_update(TextDocument::from(params))?;
                to_publish.push((uri.clone(), self.diagnostics_for_uri(&uri)));
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                let uri = params.text_document.uri.clone();
                self.handle_update(TextDocument::from(params))?;
                to_publish.push((uri.clone(), self.diagnostics_for_uri(&uri)));
            }
            DidCloseTextDocument::METHOD => {
                if let Ok(params) =
                    serde_json::from_value::<lsp_types::DidCloseTextDocumentParams>(not.params)
                {
                    let uri = params.text_document.uri.clone();
                    self.open_documents.remove(&uri);
                    to_publish.push((uri, Vec::new()));
                }
            }
            DidChangeWatchedFiles::METHOD => {
                let params: lsp_types::DidChangeWatchedFilesParams =
                    serde_json::from_value(not.params)?;
                for change in params.changes {
                    self.handle_watched_file_change(change);
                }
            }
            DidChangeWorkspaceFolders::METHOD => {
                let params: lsp_types::DidChangeWorkspaceFoldersParams =
                    serde_json::from_value(not.params)?;
                self.handle_workspace_folders_change(params);
            }
            _ => {}
        }
        Ok(to_publish)
    }

    /// Apply one watched-file change to the index. `*.circom` only. Created → intern; Deleted →
    /// unregister; Changed → re-read iff loaded. Deleted/Changed skip open docs (a build tool
    /// deleting/recreating/touching the file must not clobber the open doc's unsaved buffer —
    /// `didChange` is authoritative for it).
    fn handle_watched_file_change(&mut self, change: lsp_types::FileEvent) {
        let Ok(path) = change.uri.to_file_path() else {
            return;
        };
        if path.extension().and_then(|e| e.to_str()) != Some("circom") {
            return;
        }
        match change.typ {
            FileChangeType::CREATED => {
                // Skip open docs: a build tool deleting+recreating a file must not clobber an open
                // doc's dirty buffer (`didChange` is authoritative for it) — mirrors DELETED/CHANGED.
                if self.open_documents.contains(&change.uri) {
                    return;
                }
                if let Ok(canon) = path.canonicalize() {
                    if let Some(vpath) = vfs::VfsPath::from_abs_path(&canon) {
                        let id = self.source_db.vfs_mut().register_path(vpath.clone());
                        self.workspace_files.insert(id);
                        if let Ok(src) = std::fs::read_to_string(&path) {
                            self.source_db
                                .vfs_mut()
                                .set_file_contents(vpath, Some(Arc::from(src)));
                            self.after_vfs_mutation();
                        } else {
                            self.drop_include_cache();
                        }
                    }
                }
            }
            FileChangeType::DELETED => {
                if self.open_documents.contains(&change.uri) {
                    return;
                }
                // The file is gone, so `canonicalize` fails; absolutize and look up by identity.
                // Best-effort: a symlinked path that doesn't match the interned canonical VfsPath
                // isn't found, leaving a stale entry until the next re-walk.
                if let Some(vpath) = vfs::VfsPath::from_abs_path(&path) {
                    if let Some(id) = self.source_db.vfs().file_id(&vpath) {
                        self.workspace_files.remove(&id);
                    }
                    self.source_db.vfs_mut().unregister_path(&vpath);
                    self.after_vfs_mutation();
                }
            }
            FileChangeType::CHANGED => {
                if self.open_documents.contains(&change.uri) {
                    return;
                }
                if let Some(vpath) = vfs::VfsPath::from_abs_path(&path) {
                    let needs_reload = self
                        .source_db
                        .vfs()
                        .file_id(&vpath)
                        .is_some_and(|id| self.source_db.vfs().file_text(id).is_some());
                    if needs_reload {
                        if let Ok(src) = std::fs::read_to_string(&path) {
                            self.source_db
                                .vfs_mut()
                                .set_file_contents(vpath, Some(Arc::from(src)));
                            self.after_vfs_mutation();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Merge added/removed folders into the roots. Added → walk only that root; removed → unregister
    /// its files so they stop resolving (the pure read path can't confine them).
    fn handle_workspace_folders_change(
        &mut self,
        params: lsp_types::DidChangeWorkspaceFoldersParams,
    ) {
        let mut roots: Vec<PathBuf> = self.source_db.vfs().workspace_roots().to_vec();
        let mut added_canon: Vec<PathBuf> = Vec::new();
        for added in &params.event.added {
            if let Some(canon) = canon_folder(&added.uri) {
                if !roots.contains(&canon) {
                    roots.push(canon.clone());
                    added_canon.push(canon);
                }
            }
        }
        let mut removed_canon: Vec<PathBuf> = Vec::new();
        for removed_folder in &params.event.removed {
            if let Some(canon) = canon_folder(&removed_folder.uri) {
                roots.retain(|r| r != &canon);
                removed_canon.push(canon);
            }
        }
        self.source_db.set_workspace_roots(roots.clone());
        if !removed_canon.is_empty() {
            // Drop only files no longer under ANY remaining root — a removed folder may be nested
            // inside one, in which case its files must stay resolvable. `unregister_under` is too
            // blunt (it would null their text), so unregister the uncovered paths individually.
            let still_under = |id: FileId| -> bool {
                self.source_db
                    .vfs()
                    .path(id)
                    .is_some_and(|p| roots.iter().any(|r| p.as_path().starts_with(r)))
            };
            let mut to_drop: Vec<FileId> = Vec::new();
            for canon in &removed_canon {
                for id in self.source_db.vfs().ids_under(canon) {
                    if !still_under(id) {
                        to_drop.push(id);
                    }
                }
            }
            for id in &to_drop {
                self.workspace_files.remove(id);
                if let Some(p) = self.source_db.vfs().path(*id).cloned() {
                    self.source_db.vfs_mut().unregister_path(&p);
                }
            }
            if !to_drop.is_empty() {
                self.after_vfs_mutation();
            }
        }
        if !added_canon.is_empty() {
            self.register_indexed_paths(crate::project_index::collect_circom_files_with_content(
                &added_canon,
            ));
        }
    }

    /// Goto-definition shaper: an include-path string routes to [`include_target_location`]; any other token
    /// resolves via [`Self::resolve_token`], file-tagged.
    pub fn lookup_definition(&self, file_db: &FileDB, token: &SyntaxToken) -> Vec<Location> {
        if token.kind() == TokenKind::CircomString {
            return include_target_location(file_db, token, &self.source_db);
        }
        self.to_locations(self.resolve_token(file_db, token))
    }

    /// Resolve `token`: a component field (`c.x`) via [`Self::resolve_member`]; else [`Self::resolve_use`].
    pub(crate) fn resolve_token(
        &self,
        origin: &FileDB,
        token: &SyntaxToken,
    ) -> Vec<(FileId, ResolvedSymbol)> {
        if resolver::component_field(token).is_some() {
            self.resolve_member(origin, token)
        } else {
            self.resolve_use(origin, token)
        }
    }

    /// File-tagged resolved declarations → file-tagged LSP [`Location`]s (at the name token).
    fn to_locations(&self, resolved: Vec<(FileId, ResolvedSymbol)>) -> Vec<Location> {
        resolved
            .into_iter()
            .map(|(id, s)| Location::new(self.source_db.file_db(id).file_path.clone(), s.def_range))
            .collect()
    }

    /// The [`FileId`]s of every text-bearing include loaded for `origin`. Path-only ids (interned
    /// by the walk but unread) are filtered: `symbol_table` parses the result and would panic on a
    /// None-text id. Memoized per origin; cleared by [`Self::drop_include_cache`].
    fn loaded_includes(&self, origin: &FileDB) -> Vec<FileId> {
        if let Some(cached) = self.loaded_includes_cache.borrow().get(&origin.file_id) {
            return cached.clone();
        }
        let result = self.compute_loaded_includes(origin);
        self.loaded_includes_cache
            .borrow_mut()
            .insert(origin.file_id, result.clone());
        result
    }

    fn compute_loaded_includes(&self, origin: &FileDB) -> Vec<FileId> {
        let Some(ast) = self.source_db.ast(origin.file_id) else {
            return Vec::new();
        };
        ast.include_paths()
            .into_iter()
            .filter_map(|path| {
                let id = self.source_db.id_for_include(&origin.file_path, &path)?;
                let has_text = self.source_db.vfs().file_text(id).is_some();
                has_text.then_some(id)
            })
            .collect()
    }

    /// In-file resolution of `token` against `origin`'s own `SymbolTable` (file-tagged).
    fn resolve_in_file(
        &self,
        origin: &FileDB,
        token: &SyntaxToken,
    ) -> Vec<(FileId, ResolvedSymbol)> {
        let table = self.source_db.symbol_table(origin.file_id);
        resolver::resolve(&table, token)
            .into_iter()
            .map(|s| (origin.file_id, s))
            .collect()
    }

    /// Each **direct** include's top-level declarations named like `token` (circom's non-transitive
    /// include visibility), file-tagged with the include's `FileId`.
    fn resolve_in_includes(
        &self,
        origin: &FileDB,
        token: &SyntaxToken,
    ) -> Vec<(FileId, ResolvedSymbol)> {
        let name = token.text();
        let mut out = Vec::new();
        for lib_id in self.loaded_includes(origin) {
            let lib_table = self.source_db.symbol_table(lib_id);
            for sym in lib_table.lookup_top_level(name) {
                out.push((lib_id, ResolvedSymbol::from(sym)));
            }
        }
        out
    }

    /// Resolve `token` to its file-tagged declaration(s) for goto-def/hover: in-file first
    /// (shadowing); if empty, for a component decl/call — or the top-level `component main = X()`
    /// instantiation — each loaded include's top-level by name. `MainComponent` is a distinct node
    /// kind from `ComponentDecl`/`ComponentCall`, so it is listed explicitly (without it,
    /// `component main = Lib()` where `Lib` is in an include would never resolve).
    pub(crate) fn resolve_use(
        &self,
        origin: &FileDB,
        token: &SyntaxToken,
    ) -> Vec<(FileId, ResolvedSymbol)> {
        let in_file = self.resolve_in_file(origin, token);
        if !in_file.is_empty() {
            return in_file;
        }
        let is_component_use = token_ancestors(token).any(|n| {
            AstComponentDecl::can_cast(n.kind())
                || AstComponentCall::can_cast(n.kind())
                || AstMainComponent::can_cast(n.kind())
        });
        if is_component_use {
            self.resolve_in_includes(origin, token)
        } else {
            Vec::new()
        }
    }

    /// Resolve `token` with workspace visibility — [`Self::resolve_use`] **without** the
    /// component-use gate: in-file first (precedence/shadowing); when non-empty it wins outright, so
    /// an in-file def shadows an include's same-named def; otherwise every direct include's
    /// top-level by name. References/rename need this ungated path to find usages of *any* included
    /// top-level symbol (a template/function referenced by name inside a body), not just component
    /// instantiations.
    pub(crate) fn resolve_visible(
        &self,
        origin: &FileDB,
        token: &SyntaxToken,
    ) -> Vec<(FileId, ResolvedSymbol)> {
        let in_file = self.resolve_in_file(origin, token);
        if !in_file.is_empty() {
            return in_file;
        }
        self.resolve_in_includes(origin, token)
    }

    /// Resolve a component member-access **field** token (`c.x` / `T()(...).x`) to its signal
    /// declaration in the receiver's template — the type-inference path the flat [`resolve`]
    /// deliberately omits. Returns the file-tagged signal declaration(s), or empty if the receiver
    /// isn't a (instantiated) component, its template isn't found, or the field isn't one of its
    /// signals. Shared by goto-definition and hover. References/rename of fields are intentionally
    /// NOT handled here (they need per-template occurrence search) and remain no-ops.
    pub(crate) fn resolve_member(
        &self,
        origin: &FileDB,
        field: &SyntaxToken,
    ) -> Vec<(FileId, ResolvedSymbol)> {
        let Some(call) = resolver::component_field(field) else {
            return Vec::new();
        };
        let Some(receiver) = resolver::receiver_of(&call) else {
            return Vec::new();
        };
        let Some(recv_tok) = resolver::first_identifier(&receiver) else {
            return Vec::new();
        };

        // Template name: for an anonymous instantiation the receiver's callee IS the template; for
        // a named component, look the receiver up as a component to get its instantiated template.
        let table = self.source_db.symbol_table(origin.file_id);
        let template_name = if resolver::contains_call(&receiver) {
            recv_tok.text().to_string()
        } else {
            match table.component_type_at(recv_tok.text_range().start(), recv_tok.text()) {
                Some(t) => t.to_string(),
                None => return Vec::new(),
            }
        };

        let Some(template_file) = self.resolve_template_file(origin, &template_name) else {
            return Vec::new();
        };
        let field_name = field.text();
        self.source_db
            .symbol_table(template_file)
            .members_of(&template_name)
            .into_iter()
            .filter(|s| s.name == field_name)
            .map(|s| (template_file, s.into()))
            .collect()
    }

    /// The file defining top-level `name`: `origin` if it declares it, else the first loaded
    /// include that does. Used by member completion to find a component's template (may live in a
    /// lib).
    pub(crate) fn resolve_template_file(&self, origin: &FileDB, name: &str) -> Option<FileId> {
        if !self
            .source_db
            .symbol_table(origin.file_id)
            .lookup_top_level(name)
            .is_empty()
        {
            return Some(origin.file_id);
        }
        self.loaded_includes(origin).into_iter().find(|lib_id| {
            !self
                .source_db
                .symbol_table(*lib_id)
                .lookup_top_level(name)
                .is_empty()
        })
    }

    /// Every occurrence of `target` across the workspace, as `(FileId, Range)`. Scans
    /// [`Self::workspace_files`] plus the cursor's `origin` file and the target's defining file
    /// (the latter two cover the no-index / in-file case). For each candidate file, finds
    /// `Identifier` tokens named `target.name` and keeps those that [`Self::resolve_visible`]
    /// resolves to `(target FileId, kind, def_range)`. Resolution-based, so shadowing and same-name
    /// collisions across files are sound: a token matches only if it genuinely refers to `target`.
    /// Files with no loaded text or no parse are skipped (the `cursor_context` None-text guard,
    /// workspace-wide).
    pub(crate) fn workspace_occurrences(
        &self,
        target: &(FileId, ResolvedSymbol),
        origin: FileId,
    ) -> Vec<(FileId, Range)> {
        let (def_file, sym) = target;
        let name = sym.name.as_str();

        // Prune to files that actually contain an identifier with this text (plus the cursor's
        // file and the target's defining file), instead of scanning every workspace file.
        let mut scan = self.files_containing_name(name);
        scan.insert(origin);
        scan.insert(*def_file);

        let mut out = Vec::new();
        for f in scan {
            if self.source_db.vfs().file_text(f).is_none() {
                continue;
            }
            let file_db = self.source_db.file_db(f);
            // A token in `f` can only resolve to `target` if `f` IS the defining file, is the
            // cursor's file, or directly includes the defining file (circom's non-transitive
            // visibility). Skip the rest — they can't reference `target`, so walking their tokens
            // (an O(file_text) walk each) is wasted work on large workspaces.
            let visible =
                f == *def_file || f == origin || self.loaded_includes(&file_db).contains(def_file);
            if !visible {
                continue;
            }
            let Some(ast) = self.source_db.ast(f) else {
                continue;
            };
            for tok in resolver::identifiers_named(ast.syntax(), name) {
                let matched = self
                    .resolve_visible(&file_db, &tok)
                    .into_iter()
                    .any(|(fid, r)| {
                        fid == *def_file && r.kind == sym.kind && r.def_range == sym.def_range
                    });
                if matched {
                    out.push((f, file_db.token_range(&tok)));
                }
            }
        }
        out
    }

    /// The set of workspace files containing at least one `Identifier` token with text `name`.
    /// Backed by the cached identifier index (rebuilt lazily, dropped on any mutation).
    fn files_containing_name(&self, name: &str) -> HashSet<FileId> {
        self.ensure_identifier_index();
        self.identifier_index
            .borrow()
            .as_ref()
            .and_then(|idx| idx.get(name))
            .cloned()
            .unwrap_or_default()
    }

    /// Build the identifier index once and cache it; no-op if already built.
    fn ensure_identifier_index(&self) {
        if self.identifier_index.borrow().is_some() {
            return;
        }
        let mut index: HashMap<String, HashSet<FileId>> = HashMap::new();
        for f in &self.workspace_files {
            if self.source_db.vfs().file_text(*f).is_none() {
                continue;
            }
            let Some(ast) = self.source_db.ast(*f) else {
                continue;
            };
            for tok in ast
                .syntax()
                .descendants_with_tokens()
                .filter_map(|e| e.into_token())
            {
                if tok.kind() == TokenKind::Identifier {
                    index.entry(tok.text().to_string()).or_default().insert(*f);
                }
            }
        }
        *self.identifier_index.borrow_mut() = Some(index);
    }

    /// Every top-level template/function/bus in the workspace whose name matches `query` (empty
    /// query ⇒ all), as owned `(FileId, Symbol)` (each file's `SymbolTable` is cached behind a
    /// short-lived borrow). Powers `workspace/symbol`.
    pub(crate) fn workspace_symbols(&self, query: &str) -> Vec<(FileId, Symbol)> {
        let q = query.trim().to_lowercase();
        let mut out = Vec::new();
        for f in &self.workspace_files {
            if self.source_db.vfs().file_text(*f).is_none() {
                continue;
            }
            if self.source_db.ast(*f).is_none() {
                continue;
            }
            let table = self.source_db.symbol_table(*f);
            for sym in table.top_level_symbols() {
                if is_workspace_symbol_kind(sym.kind) && matches_query(&q, &sym.name) {
                    out.push((*f, sym.clone()));
                }
            }
        }
        out
    }

    /// Register an updated document: set text (dropping derived caches) and load each include once.
    /// Identical text short-circuits; non-`file:` URIs and unreadable includes are skipped, not
    /// crashed. Takes the doc by value so `text` moves.
    pub fn handle_update(&mut self, text_document: TextDocument) -> Result<()> {
        self.open_documents.insert(text_document.uri.clone());

        let Some((id, changed)) = self
            .source_db
            .set_document(&text_document.uri, text_document.text)
        else {
            // Non-`file:` scheme (untitled/git) — nothing to index.
            return Ok(());
        };

        // Identical text → nothing changed, so the parse cache and symbol table are still valid.
        if !changed {
            return Ok(());
        }

        // An edit can change which includes are loaded → drop the resolved-include memo.
        self.drop_include_cache();

        // Includes load from disk once then cache; symbol tables build lazily on first query.
        if let Some(ast) = self.source_db.ast(id) {
            for path in ast.include_paths() {
                let _ = self.source_db.load_include(&text_document.uri, &path);
            }
        }

        Ok(())
    }
}

/// Canonical absolute path of a workspace-folder URI (`None` for non-`file:` or un-canonicalizable).
fn canon_folder(uri: &Url) -> Option<PathBuf> {
    uri.to_file_path().ok().and_then(|p| p.canonicalize().ok())
}

/// `true` for top-level kinds surfaced by `workspace/symbol` (templates/functions/buses).
fn is_workspace_symbol_kind(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Template | SymbolKind::Function | SymbolKind::Bus
    )
}

/// Case-insensitive substring match against an already-lowercased `query` (callers hoist the
/// `to_lowercase` once per request); an empty query matches everything.
fn matches_query(query_lower: &str, name: &str) -> bool {
    query_lower.is_empty() || name.to_lowercase().contains(query_lower)
}

/// Deserialize params, run the handler, wrap the result in a success `Response`. Generic so
/// [`GlobalState::handle_request`] stays a flat one-line-per-feature table.
fn dispatch<P, R>(
    state: &GlobalState,
    id: RequestId,
    req: Request,
    handle: fn(&GlobalState, P) -> Result<R>,
) -> Result<Option<Response>>
where
    P: serde::de::DeserializeOwned,
    R: serde::Serialize,
{
    let params: P = serde_json::from_value(req.params)?;
    let result = handle(state, params)?;
    Ok(Some(Response {
        id,
        result: Some(serde_json::to_value(&result)?),
        error: None,
    }))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::Url;

    use crate::source_db::SourceDatabase;

    use super::{GlobalState, TextDocument};

    /// A `TextDocument` from URI + text (private fields, but this test module is inside
    /// `global_state`).
    fn doc(uri: &Url, text: String) -> TextDocument {
        TextDocument {
            text,
            uri: uri.clone(),
        }
    }

    /// Absolute path to a fixture under `src/test_files/`, resolved from `CARGO_MANIFEST_DIR`.
    fn fixture_uri(rel: &str) -> Url {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = Path::new(&crate_path).join(format!("src/test_files/handler/{rel}"));
        Url::from_file_path(&path).unwrap()
    }

    /// A clean program yields no diagnostics; a missing `;` yields an ERROR diagnostic whose
    /// message references the expected token.
    #[test]
    fn diagnostics_for_clean_and_broken_docs_test() {
        let mut state = GlobalState::new(Vec::new());

        let clean = Url::from_file_path("/tmp/diag_clean.circom").unwrap();
        state
            .source_db
            .set_document(&clean, "pragma circom 2.0.0;\n".to_string());
        assert!(
            state.diagnostics_for_uri(&clean).is_empty(),
            "clean file should have no diagnostics"
        );

        let broken = Url::from_file_path("/tmp/diag_broken.circom").unwrap();
        state
            .source_db
            .set_document(&broken, "pragma circom 2.0.0".to_string());
        let diags = state.diagnostics_for_uri(&broken);
        assert_eq!(diags.len(), 1, "missing semicolon: {diags:?}");
        assert_eq!(
            diags[0].severity,
            Some(lsp_types::DiagnosticSeverity::ERROR)
        );
        assert_eq!(diags[0].source.as_deref(), Some("circom"));
        assert!(
            diags[0].message.contains("Semicolon"),
            "{}",
            diags[0].message
        );
    }

    /// A textless/unknown file yields no diagnostics and never panics.
    #[test]
    fn diagnostics_for_unknown_uri_is_empty_test() {
        let state = GlobalState::new(Vec::new());
        let unknown = Url::from_file_path("/tmp/nope.circom").unwrap();
        assert!(state.diagnostics_for_uri(&unknown).is_empty());
    }

    /// Editing the main file must never re-read or re-parse an unchanged `include`:
    /// `parse_count` for the lib stays at 1 across a main-file `didChange`. (Indexing is lazy — the
    /// lib parses on first query, triggered here by an explicit `ast` query.)
    #[test]
    fn include_parsed_once_across_keystrokes_test() {
        let main_uri = fixture_uri("with_include/main.circom");
        let lib_uri = fixture_uri("with_include/lib.circom");
        let src = std::fs::read_to_string(main_uri.to_file_path().unwrap()).unwrap();

        // Seed the fixture dir as a workspace root (path-traversal confinement for the include).
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let root = Path::new(&crate_path)
            .join("src/test_files/handler/with_include")
            .canonicalize()
            .unwrap();
        let mut state = GlobalState::new(vec![root]);

        // First open: registers main + loads lib text into the VFS (no parse yet — indexing is
        // lazy).
        state.handle_update(doc(&main_uri, src.clone())).unwrap();
        let lib_id = state
            .source_db
            .id_for_url(&lib_uri)
            .expect("include should be loaded on first open");

        // First query touching the lib parses it once.
        let _ = state.source_db.ast(lib_id);
        assert_eq!(
            state.source_db.parse_count(lib_id),
            1,
            "lib parsed exactly once on first query"
        );

        // Second main-file didChange (text differs) — the lib is a cache hit: not re-read or
        // re-parsed.
        let src2 = format!("{src}\n// a trailing edit\n");
        state.handle_update(doc(&main_uri, src2)).unwrap();
        assert_eq!(
            state.source_db.parse_count(lib_id),
            1,
            "lib must not reparse on a main-file keystroke"
        );
    }

    /// A non-`file:` document (untitled) is skipped without panicking and isn't indexed.
    #[test]
    fn handle_update_skips_non_file_uri_test() {
        let mut state = GlobalState::new(Vec::new());
        let untitled = Url::parse("untitled:Untitled-1").unwrap();
        // Must not panic, and must not register the document.
        state
            .handle_update(doc(&untitled, "pragma circom 2.0.0;".to_string()))
            .unwrap();
        assert!(state.source_db.id_for_url(&untitled).is_none());
    }

    /// Regression: a same-dir `include "lib.circom"` resolves **without** `index_workspace` — the
    /// basename index is an addition, not a gate, so the pre-index behavior is unchanged.
    #[test]
    fn sibling_include_resolves_without_index_test() {
        let main_uri = fixture_uri("with_include/main.circom");
        let lib_uri = fixture_uri("with_include/lib.circom");
        let src = std::fs::read_to_string(main_uri.to_file_path().unwrap()).unwrap();
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let root = Path::new(&crate_path)
            .join("src/test_files/handler/with_include")
            .canonicalize()
            .unwrap();
        let mut state = GlobalState::new(vec![root]);
        // NOTE: no index_workspace() — same-dir resolution must work exactly as before.
        state.handle_update(doc(&main_uri, src)).unwrap();
        assert!(
            state.source_db.id_for_url(&lib_uri).is_some(),
            "sibling include loads without the basename index"
        );
    }

    /// A non-sibling `include "lib.circom"` (lib under `circuits/`) resolves via the basename index
    /// after `index_workspace`: the same-dir lookup misses, `find_include` picks the indexed
    /// `circuits/lib.circom`, and `load_include` reads it. `id_for_include` then surfaces it for
    /// cross-file resolve.
    #[test]
    fn nonsibling_include_resolves_via_index_test() {
        let main_uri = fixture_uri("with_include_nonsibling/main.circom");
        let src = std::fs::read_to_string(main_uri.to_file_path().unwrap()).unwrap();
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let root = Path::new(&crate_path)
            .join("src/test_files/handler/with_include_nonsibling")
            .canonicalize()
            .unwrap();
        let mut state = GlobalState::new(vec![root]);
        state.index_workspace();
        state.handle_update(doc(&main_uri, src)).unwrap();

        // The lib lives under circuits/ — a same-dir lookup would miss, so loading proves the
        // basename fallback fired.
        let lib_id = state
            .source_db
            .load_include(&main_uri, "lib.circom")
            .expect("non-sibling include must resolve via the basename index");
        assert!(
            state.source_db.vfs().file_text(lib_id).is_some(),
            "the resolved lib has loaded text"
        );

        // `id_for_include` (the pure path used by cross-file resolve) also finds it.
        assert_eq!(
            state.source_db.id_for_include(&main_uri, "lib.circom"),
            Some(lib_id),
            "id_for_include surfaces the same winner as load_include"
        );
    }

    /// With two files sharing the basename `lib.circom`, an `include "circuits/lib.circom"` resolves
    /// to the suffix-matching candidate (`circuits/lib.circom`), not the decoy (`other/lib.circom`).
    #[test]
    fn duplicate_basename_picks_suffix_match_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_dup_{}", std::process::id()));
        let ws = base.join("ws");
        let circuits = ws.join("circuits");
        let other = ws.join("other");
        fs::create_dir_all(&circuits).unwrap();
        fs::create_dir_all(&other).unwrap();
        let lib_body = "pragma circom 2.0.0;\ntemplate Lib() { signal output o; o <== 0; }\n";
        fs::write(circuits.join("lib.circom"), lib_body).unwrap();
        fs::write(other.join("lib.circom"), lib_body).unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"circuits/lib.circom\";\n";
        fs::write(ws.join("main.circom"), main_src).unwrap();

        let main_url = Url::from_file_path(ws.join("main.circom")).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace();

        let id = state
            .source_db
            .load_include(&main_url, "circuits/lib.circom")
            .expect("include resolves among duplicate basenames");
        let resolved = state
            .source_db
            .vfs()
            .path(id)
            .expect("winner is interned")
            .as_path()
            .to_path_buf();
        assert!(
            resolved.ends_with("circuits/lib.circom"),
            "suffix match wins over the decoy: {resolved:?}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Regression (CRITICAL fix): an interned file with **no text** must NOT crash
    /// `cursor_context`/`file_text().expect()`. `id_for_url` returns `Some` for the interned id;
    /// the query path must treat it as unreadable and return `None`. (Eager loading means
    /// `index_workspace` no longer produces text-less ids, so this interns a path-only id directly
    /// to exercise the guard.)
    #[test]
    fn cursor_context_no_panic_on_textless_file_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_panic_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("main.circom"), "pragma circom 2.0.0;\n").unwrap();
        let canon = ws.join("main.circom").canonicalize().unwrap();

        let url = Url::from_file_path(&canon).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        // Intern the path with NO text (path-only), simulating a not-yet-loaded/leaked id.
        if let Some(vpath) = vfs::VfsPath::from_abs_path(&canon) {
            state.source_db.vfs_mut().register_path(vpath);
        }

        assert!(
            state.source_db.id_for_url(&url).is_some(),
            "file is interned"
        );
        assert!(
            state
                .cursor_context(&url, lsp_types::Position::new(0, 0))
                .is_none(),
            "None-text file must not crash cursor_context"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Regression (dirty-buffer fix): a `workspace/didChangeWatchedFiles` CHANGED event for a file
    /// the client has open must NOT overwrite its in-memory (dirty) text from disk — `didChange` is
    /// authoritative for open docs.
    #[test]
    fn watched_changed_skips_open_document_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_open_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_path = ws.join("lib.circom");
        fs::write(&lib_path, "pragma circom 2.0.0;\n").unwrap();

        let lib_url = Url::from_file_path(&lib_path).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        // Open with dirty text not yet on disk.
        state
            .handle_update(doc(
                &lib_url,
                "pragma circom 2.0.0;\ntemplate Dirty() {}\n".to_string(),
            ))
            .unwrap();

        // Disk content changes underneath; watcher fires CHANGED.
        fs::write(&lib_path, "pragma circom 2.0.0;\n").unwrap();
        state.handle_watched_file_change(lsp_types::FileEvent {
            uri: lib_url.clone(),
            typ: lsp_types::FileChangeType::CHANGED,
        });

        // The open doc's dirty text is preserved (reload was skipped).
        let id = state.source_db.id_for_url(&lib_url).unwrap();
        let text = state.source_db.file_text(id);
        assert!(
            text.contains("Dirty"),
            "open-doc buffer must be preserved across a watcher CHANGED: {text}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Regression (CREATED fix): a `workspace/didChangeWatchedFiles` CREATED event for a file the
    /// client has open must NOT overwrite its dirty buffer. A build tool that deletes+recreates a
    /// file while it's open fires CREATED; the open doc's unsaved edits must survive (mirrors the
    /// CHANGED/DELETED open-doc guards).
    #[test]
    fn watched_created_skips_open_document_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_created_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_path = ws.join("lib.circom");
        let lib_url = Url::from_file_path(&lib_path).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        // Open with dirty text; the file does not yet exist on disk.
        state
            .handle_update(doc(
                &lib_url,
                "pragma circom 2.0.0;\ntemplate Dirty() {}\n".to_string(),
            ))
            .unwrap();

        // The file is (re)created on disk underneath; watcher fires CREATED.
        fs::write(&lib_path, "pragma circom 2.0.0;\n").unwrap();
        state.handle_watched_file_change(lsp_types::FileEvent {
            uri: lib_url.clone(),
            typ: lsp_types::FileChangeType::CREATED,
        });

        let id = state.source_db.id_for_url(&lib_url).unwrap();
        let text = state.source_db.file_text(id);
        assert!(
            text.contains("Dirty"),
            "open-doc buffer must be preserved across a watcher CREATED: {text}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Regression (DELETED fix): a `workspace/didChangeWatchedFiles` DELETED event for a file the
    /// client has open must NOT null its in-memory text (a build tool deleting+recreating the file
    /// must not make the open doc unresolvable). Mirrors the CHANGED open-doc guard.
    #[test]
    fn watched_deleted_skips_open_document_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_del_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        let lib_path = ws.join("lib.circom");
        fs::write(&lib_path, "pragma circom 2.0.0;\n").unwrap();
        let lib_url = Url::from_file_path(&lib_path).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state
            .handle_update(doc(
                &lib_url,
                "pragma circom 2.0.0;\ntemplate Dirty() {}\n".to_string(),
            ))
            .unwrap();

        // File deleted on disk while the doc is open; watcher fires DELETED.
        let _ = fs::remove_file(&lib_path);
        state.handle_watched_file_change(lsp_types::FileEvent {
            uri: lib_url.clone(),
            typ: lsp_types::FileChangeType::DELETED,
        });

        // The open doc's text is preserved (the unregister was skipped).
        let id = state.source_db.id_for_url(&lib_url).unwrap();
        let text = state.source_db.file_text(id);
        assert!(
            text.contains("Dirty"),
            "open-doc text must be preserved across a watcher DELETED: {text}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Regression (removed-folder fix): after a workspace folder is removed, an already-loaded
    /// include that lived under it must stop resolving (the read path now checks confinement).
    #[test]
    fn removed_folder_include_stops_resolving_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_rm_{}", std::process::id()));
        let ws1 = base.join("ws1");
        let ws2 = base.join("ws2");
        fs::create_dir_all(&ws1).unwrap();
        fs::create_dir_all(&ws2).unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\n";
        fs::write(ws1.join("main.circom"), main_src).unwrap();
        fs::write(
            ws2.join("lib.circom"),
            "pragma circom 2.0.0;\ntemplate Lib() { signal output o; o <== 0; }\n",
        )
        .unwrap();
        let main_url = Url::from_file_path(ws1.join("main.circom")).unwrap();
        let mut state = GlobalState::new(vec![
            ws1.canonicalize().unwrap(),
            ws2.canonicalize().unwrap(),
        ]);
        state.index_workspace();
        state
            .handle_update(doc(&main_url, main_src.to_string()))
            .unwrap();
        // The non-sibling lib (under ws2) resolves via the basename fallback.
        assert!(
            state
                .source_db
                .id_for_include(&main_url, "lib.circom")
                .is_some(),
            "lib resolves while its root is present"
        );

        // Remove ws2 from the workspace.
        state.handle_workspace_folders_change(lsp_types::DidChangeWorkspaceFoldersParams {
            event: lsp_types::WorkspaceFoldersChangeEvent {
                added: Vec::new(),
                removed: vec![lsp_types::WorkspaceFolder {
                    uri: Url::from_file_path(&ws2).unwrap(),
                    name: "ws2".to_string(),
                }],
            },
        });

        // The lib is no longer confined → must stop resolving.
        assert!(
            state
                .source_db
                .id_for_include(&main_url, "lib.circom")
                .is_none(),
            "removed-folder include must stop resolving"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Regression (nested-folder fix): removing a folder nested inside a *remaining* root must NOT
    /// drop files that are still covered by the outer root. Previously `unregister_under` nulled
    /// their text, silently breaking resolution.
    #[test]
    fn removed_nested_folder_keeps_covered_files_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_nested_{}", std::process::id()));
        let outer = base.join("proj");
        let inner = outer.join("sub");
        fs::create_dir_all(&inner).unwrap();
        fs::write(
            inner.join("lib.circom"),
            "pragma circom 2.0.0;\ntemplate Lib() { signal output o; o <== 0; }\n",
        )
        .unwrap();
        let lib_url = Url::from_file_path(inner.join("lib.circom")).unwrap();

        // Both the outer project and its nested sub-folder are roots.
        let mut state = GlobalState::new(vec![
            outer.canonicalize().unwrap(),
            inner.canonicalize().unwrap(),
        ]);
        state.index_workspace();
        let id = state.source_db.id_for_url(&lib_url).expect("lib indexed");
        assert!(
            state.source_db.vfs().file_text(id).is_some(),
            "lib has loaded text"
        );

        // Remove only the nested folder — `outer` still covers `lib.circom`.
        state.handle_workspace_folders_change(lsp_types::DidChangeWorkspaceFoldersParams {
            event: lsp_types::WorkspaceFoldersChangeEvent {
                added: Vec::new(),
                removed: vec![lsp_types::WorkspaceFolder {
                    uri: Url::from_file_path(&inner).unwrap(),
                    name: "sub".to_string(),
                }],
            },
        });

        // The file is still under the outer root → it must keep its text + stay in the workspace.
        assert!(
            state.source_db.vfs().file_text(id).is_some(),
            "a file still under a remaining root must keep its text"
        );
        assert!(
            state.workspace_files.contains(&id),
            "a file still under a remaining root stays in workspace_files"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// Regression (walk-landing reload fix): a doc with a non-sibling include opened BEFORE the
    /// index is populated has its include loaded when the walk lands — without needing another edit
    /// — so cross-file symbol features activate immediately.
    #[test]
    fn index_landing_reloads_open_doc_non_sibling_include_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_land_{}", std::process::id()));
        let ws = base.join("ws");
        let circuits = ws.join("circuits");
        fs::create_dir_all(&circuits).unwrap();
        fs::write(
            circuits.join("lib.circom"),
            "pragma circom 2.0.0;\ntemplate Lib() { signal output o; o <== 0; }\n",
        )
        .unwrap();
        let main_src = "pragma circom 2.0.0;\ninclude \"lib.circom\";\ncomponent main = Lib();\n";
        fs::write(ws.join("main.circom"), main_src).unwrap();
        let main_url = Url::from_file_path(ws.join("main.circom")).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);

        // Open main BEFORE the index is built (simulates didOpen beating the background walk).
        state
            .handle_update(doc(&main_url, main_src.to_string()))
            .unwrap();
        assert!(
            state
                .source_db
                .id_for_include(&main_url, "lib.circom")
                .is_none(),
            "pre-index: non-sibling include is unresolved"
        );

        // The walk lands → open docs' includes are re-loaded.
        state.index_workspace();
        assert!(
            state
                .source_db
                .id_for_include(&main_url, "lib.circom")
                .is_some(),
            "post-index: include resolves after the walk reloads open docs"
        );

        let _ = fs::remove_dir_all(&base);
    }
}
