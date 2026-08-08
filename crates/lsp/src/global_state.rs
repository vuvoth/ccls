use anyhow::Result;
use lsp_server::{Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidChangeWorkspaceFolders, DidCloseTextDocument,
    DidOpenTextDocument, Notification as _,
};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest,
    PrepareRenameRequest, References, Rename, Request as _,
};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, FileChangeType, Location, Range, Url,
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
use crate::handler::goto_definition::jump_to_lib;
use crate::resolver::{self, token_ancestors, ResolvedSymbol};
use crate::source_db::{ContentCacheDb, SourceDatabase};

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
    /// URIs of documents the client has open (didOpen, not yet didClose). The file-watcher's
    /// `CHANGED` arm skips these so an external disk touch never clobbers a document's unsaved
    /// (dirty) text — for an open doc, `didChange` is authoritative.
    open_documents: HashSet<Url>,
    /// Memoized result of [`Self::loaded_includes`] per origin [`FileId`]. Interior-mutable so the
    /// `&self` read path can fill it; cleared by [`Self::drop_include_cache`] on any text/index
    /// mutation (an edit, a watched create/delete, a workspace walk result) so it can't go stale.
    loaded_includes_cache: RefCell<HashMap<FileId, Vec<FileId>>>,
}

/// Internal (non-LSP) notification method the backgrounded workspace walker uses to hand its
/// collected paths back to the main loop, so the eager index build never blocks `initialize`.
pub(crate) const INDEX_WORKSPACE_RESULT_METHOD: &str = "ccls/indexWorkspaceResult";

/// Payload of [`INDEX_WORKSPACE_RESULT_METHOD`]: the canonical `.circom` paths the background
/// walker collected. Deserialized on the main thread and fed to `register_indexed_paths`.
#[derive(serde::Deserialize)]
struct IndexWorkspaceResult {
    paths: Vec<PathBuf>,
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
    /// Construct with workspace `roots` confining `include` resolution. Empty roots ⇒ no include
    /// ever loads (fail-closed against path traversal).
    pub fn new(roots: Vec<PathBuf>) -> Self {
        let mut source_db = ContentCacheDb::new();
        source_db.set_workspace_roots(roots);
        Self {
            source_db,
            open_documents: HashSet::new(),
            loaded_includes_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Drop the memoized [`Self::loaded_includes`] entries — call after any text or basename-index
    /// mutation (an edit, a watched create/delete/change, an index walk result, a workspace-folder
    /// change) so a stale include-id list can't be served. Coarse but safe: edits are infrequent
    /// relative to reads.
    pub(crate) fn drop_include_cache(&mut self) {
        self.loaded_includes_cache.borrow_mut().clear();
    }

    /// Resolve `(uri, position)` to a [`CursorContext`], or `None` if the file is unknown, has no
    /// loaded text, or fails to parse. The no-text guard is load-bearing: the eager workspace walk
    /// interns every `.circom` path with **no text**, and a watcher `DELETED` can null an open
    /// doc's text — without this check such an id would reach `file_text().expect()` and crash the
    /// single-threaded server.
    pub(crate) fn cursor_context(
        &self,
        uri: &Url,
        position: lsp_types::Position,
    ) -> Option<CursorContext> {
        let id = self.source_db.id_for_url(uri)?;
        // No-text guard: the eager walk interns paths with no text and a watcher DELETED can null an
        // open doc's text — without this an id here would reach `file_text().expect()` and crash the
        // single-threaded server. `?` returns None for an absent-text id.
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

    /// Dispatch an LSP request to its handler by method name; `Ok(None)` for unhandled methods.
    /// Add a request = one arm here + a handler module + a capability entry.
    pub fn handle_request(&self, req: Request) -> Result<Option<Response>> {
        let id = req.id.clone();
        match req.method.as_str() {
            GotoDefinition::METHOD => dispatch(self, id, req, handler::goto_definition::handle),
            HoverRequest::METHOD => dispatch(self, id, req, handler::hover::handle),
            Completion::METHOD => dispatch(self, id, req, handler::completion::handle),
            References::METHOD => dispatch(self, id, req, handler::references::handle),
            DocumentSymbolRequest::METHOD => {
                dispatch(self, id, req, handler::document_symbol::handle)
            }
            Formatting::METHOD => dispatch(self, id, req, handler::formatting::handle),
            Rename::METHOD => dispatch(self, id, req, handler::rename::handle),
            PrepareRenameRequest::METHOD => dispatch(self, id, req, handler::rename::prepare),
            _ => Ok(None),
        }
    }

    /// Dispatch an LSP notification. Document open/change load includes; didClose drops open-doc
    /// tracking; watched-file changes keep the project basename index fresh (created/deleted
    /// `.circom` files); workspace-folder changes walk the newly-added roots.
    pub fn handle_notification(&mut self, not: Notification) -> Result<()> {
        match not.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                self.handle_update(TextDocument::from(params))?;
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                self.handle_update(TextDocument::from(params))?;
            }
            DidCloseTextDocument::METHOD => {
                // No params needed beyond the uri; just stop tracking it as open so a later disk
                // change can reload it. Deserialize to validate shape; ignore parse errors softly.
                if let Ok(params) =
                    serde_json::from_value::<lsp_types::DidCloseTextDocumentParams>(not.params)
                {
                    self.open_documents.remove(&params.text_document.uri);
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
            INDEX_WORKSPACE_RESULT_METHOD => {
                // Background walker (spawned at initialize) hands back the collected paths;
                // register them on the main thread (Vfs is single-threaded).
                if let Ok(result) = serde_json::from_value::<IndexWorkspaceResult>(not.params) {
                    self.register_indexed_paths(result.paths);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Apply one watched-file change to the project basename index. Only `*.circom` files matter:
    /// Created → `register_path` (intern path-only); Deleted → `unregister_path` (drop text +
    /// caches); Changed → re-read **iff** the file was already loaded AND is not currently open
    /// (an open doc's unsaved buffer stays authoritative — `didChange` re-asserts it). A never-loaded
    /// file stays path-only until an include resolves to it.
    fn handle_watched_file_change(&mut self, change: lsp_types::FileEvent) {
        let Ok(path) = change.uri.to_file_path() else {
            return;
        };
        if path.extension().and_then(|e| e.to_str()) != Some("circom") {
            return;
        }
        match change.typ {
            FileChangeType::CREATED => {
                if let Ok(canon) = path.canonicalize() {
                    if let Some(vpath) = vfs::VfsPath::from_abs_path(&canon) {
                        self.source_db.vfs_mut().register_path(vpath);
                        self.source_db.invalidate_changed();
                        self.drop_include_cache();
                    }
                }
            }
            FileChangeType::DELETED => {
                // The file is gone, so `canonicalize` fails; absolutize the reported path and look
                // it up by identity. Best-effort: a symlinked path that doesn't match the interned
                // canonical VfsPath simply isn't found, leaving a stale entry until the next re-walk.
                if let Some(vpath) = vfs::VfsPath::from_abs_path(&path) {
                    self.source_db.vfs_mut().unregister_path(&vpath);
                    self.source_db.invalidate_changed();
                    self.drop_include_cache();
                }
            }
            FileChangeType::CHANGED => {
                // Skip open documents: their authoritative text arrives via didChange, so a disk
                // touch (formatter/git) must not clobber the unsaved buffer.
                if self.open_documents.contains(&change.uri) {
                    return;
                }
                // Re-read only if already loaded (text present); a path-only file stays lazy.
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
                            self.source_db.invalidate_changed();
                            self.drop_include_cache();
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Merge added/removed workspace folders into the roots and walk **only the newly-added** roots
    /// (a full re-walk on every folder change would re-traverse all of `node_modules`). Removed
    /// folders' already-indexed files linger but become unconfined once the root is dropped, so no
    /// include will load them.
    fn handle_workspace_folders_change(
        &mut self,
        params: lsp_types::DidChangeWorkspaceFoldersParams,
    ) {
        let mut roots: Vec<PathBuf> = self.source_db.vfs().workspace_roots().to_vec();
        let mut added_canon: Vec<PathBuf> = Vec::new();
        for added in &params.event.added {
            if let Some(canon) = added
                .uri
                .to_file_path()
                .ok()
                .and_then(|p| p.canonicalize().ok())
            {
                if !roots.contains(&canon) {
                    roots.push(canon.clone());
                    added_canon.push(canon);
                }
            }
        }
        let removed = !params.event.removed.is_empty();
        for removed_folder in &params.event.removed {
            if let Some(canon) = removed_folder
                .uri
                .to_file_path()
                .ok()
                .and_then(|p| p.canonicalize().ok())
            {
                roots.retain(|r| r != &canon);
            }
        }
        self.source_db.set_workspace_roots(roots);
        // Walk only the added roots; a removed root can't add files. Drop the include cache either
        // way (confinement/roots changed, so prior resolved includes may no longer apply).
        if !added_canon.is_empty() {
            self.register_indexed_paths(crate::project_index::collect_circom_files(&added_canon));
        } else if removed {
            self.drop_include_cache();
        }
    }

    /// Goto-definition shaper: an include-path string routes to [`jump_to_lib`]; any other token
    /// resolves via [`Self::resolve_token`], file-tagged.
    pub fn lookup_definition(&self, file_db: &FileDB, token: &SyntaxToken) -> Vec<Location> {
        if token.kind() == TokenKind::CircomString {
            return jump_to_lib(file_db, token, self.source_db.vfs());
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

    /// The [`FileId`]s of every include loaded for `origin`. Shared by [`Self::resolve_use`] and
    /// [`Self::resolve_template_file`] so the include walk lives in one place. Only text-bearing
    /// ids are surfaced: `id_for_include`'s basename fallback can return a path-only id (interned
    /// by the workspace walk but not yet read), and `symbol_table` below parses its result — a
    /// None-text id would panic in `file_text`, so it's filtered out here. Memoized per origin and
    /// cleared by [`Self::drop_include_cache`] on any mutation.
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

    /// The uncached resolution behind [`Self::loaded_includes`].
    fn compute_loaded_includes(&self, origin: &FileDB) -> Vec<FileId> {
        let Some(ast) = self.source_db.ast(origin.file_id) else {
            return Vec::new();
        };
        ast.libs()
            .into_iter()
            .filter_map(|inc| inc.lib())
            .filter_map(|path| {
                let id = self
                    .source_db
                    .id_for_include(&origin.file_path, &path.value())?;
                let has_text = self.source_db.vfs().file_text(id).is_some();
                has_text.then_some(id)
            })
            .collect()
    }

    /// Resolve `token` to its file-tagged declaration(s): in-file first; then, for a component
    /// decl/call — or the top-level `component main = X()` instantiation — each loaded include's
    /// top-level by name. Cross-file is file-scope only (template/function names) — the shared
    /// core for goto-def, rename, references.
    pub(crate) fn resolve_use(
        &self,
        origin: &FileDB,
        token: &SyntaxToken,
    ) -> Vec<(FileId, ResolvedSymbol)> {
        let table = self.source_db.symbol_table(origin.file_id);
        let mut out: Vec<(FileId, ResolvedSymbol)> = resolver::resolve(&table, token)
            .into_iter()
            .map(|s| (origin.file_id, s))
            .collect();

        // A component declaration/call — or the top-level `component main = X()` instantiation —
        // also resolves to template/function defs in loaded includes. `MainComponent` is a distinct
        // node kind from `ComponentDecl`/`ComponentCall`, so it is listed explicitly (without it,
        // `component main = Lib()` where `Lib` is in an include would never resolve).
        let is_component_use = token_ancestors(token).any(|n| {
            AstComponentDecl::can_cast(n.kind())
                || AstComponentCall::can_cast(n.kind())
                || AstMainComponent::can_cast(n.kind())
        });
        if is_component_use {
            let name = token.text();
            for lib_id in self.loaded_includes(origin) {
                let lib_table = self.source_db.symbol_table(lib_id);
                for sym in lib_table.lookup_top_level(name) {
                    out.push((lib_id, sym.into()));
                }
            }
        }
        out
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

    /// Occurrences of `target` in its defining file, as tokens. In-file by design: each
    /// `SymbolTable` indexes only its own file, so a token resolves unambiguously. Cross-file rename
    /// is deferred — it needs a workspace symbol graph, not name/`def_range` matching across files
    /// (that both misses real cross-file usages and can collide when two files define a same-named
    /// symbol at the same line:column).
    pub(crate) fn find_occurrences(
        &self,
        target: &(FileId, ResolvedSymbol),
    ) -> Vec<syntax::node::SyntaxToken> {
        let (def_file, sym) = target;
        let Some(ast) = self.source_db.ast(*def_file) else {
            return Vec::new();
        };
        let table = self.source_db.symbol_table(*def_file);
        resolver::occurrences_in(ast.syntax(), &table, sym)
    }

    /// Defining-file URL + every occurrence range of `target` (shared by references and rename).
    pub(crate) fn occurrence_ranges(&self, target: &(FileId, ResolvedSymbol)) -> (Url, Vec<Range>) {
        let def_file_db = self.source_db.file_db(target.0);
        let ranges = self
            .find_occurrences(target)
            .into_iter()
            .map(|t| def_file_db.token_range(&t))
            .collect();
        (def_file_db.file_path.clone(), ranges)
    }

    /// Register an updated document: set its text (dropping derived caches) and load each `include`
    /// once. No eager index — the symbol table builds lazily and invalidates on edit; a no-op
    /// (identical text) short-circuits; non-`file:` URIs and unreadable includes are skipped, not
    /// crashed. Takes the document by value so `text` moves (not clones) — drops one `String` clone
    /// per keystroke.
    pub fn handle_update(&mut self, text_document: TextDocument) -> Result<()> {
        // Track open documents so the file-watcher never clobbers a dirty buffer (didChange is
        // authoritative for open docs).
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
            for include in ast.libs() {
                let Some(include_path) = include.lib() else {
                    continue;
                };
                let _ = self
                    .source_db
                    .load_include(&text_document.uri, &include_path.value());
            }
        }

        Ok(())
    }
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

    /// Regression (CRITICAL fix): a workspace `.circom` file that is indexed (path-only, no text)
    /// but never opened must NOT crash `cursor_context`/`file_text().expect()`. `id_for_url` returns
    /// `Some` for the indexed id; the query path must treat it as unreadable and return `None`.
    #[test]
    fn cursor_context_no_panic_on_indexed_unloaded_file_test() {
        use std::fs;
        let base = std::env::temp_dir().join(format!("ccls_panic_{}", std::process::id()));
        let ws = base.join("ws");
        fs::create_dir_all(&ws).unwrap();
        fs::write(ws.join("main.circom"), "pragma circom 2.0.0;\n").unwrap();

        let url = Url::from_file_path(ws.join("main.circom")).unwrap();
        let mut state = GlobalState::new(vec![ws.canonicalize().unwrap()]);
        state.index_workspace(); // interns main.circom with NO text

        // Indexed → id resolves; text absent → query must not panic, just decline.
        assert!(
            state.source_db.id_for_url(&url).is_some(),
            "file is indexed"
        );
        assert!(
            state
                .cursor_context(&url, lsp_types::Position::new(0, 0))
                .is_none(),
            "None-text indexed file must not crash cursor_context"
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
}
