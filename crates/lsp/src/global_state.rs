use anyhow::Result;
use lsp_server::{Notification, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification as _};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest, References,
    Rename, Request as _,
};
use lsp_types::{DidChangeTextDocumentParams, DidOpenTextDocumentParams, Location, Url};
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use rowan::TextSize;
use syntax::abstract_syntax_tree::{AstCircomProgram, AstComponentCall, AstComponentDecl};
use syntax::syntax_node::SyntaxToken;

use std::path::PathBuf;

use crate::file_db::{FileDB, FileId};
use crate::handler;
use crate::handler::goto_definition::jump_to_lib;
use crate::resolver::{self, token_ancestors, ResolvedSymbol};
use crate::source_db::{ContentCacheDb, SourceDatabase};

/// A text document notification (`textDocument/didOpen` or `textDocument/didChange`) normalized to
/// its full text + URI, regardless of which notification carried it.
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
        // A `didChange` may legally carry an empty `contentChanges` array; index it safely rather
        // than panicking (a panic would kill the single-threaded server).
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

/// Server-wide state shared by every request/notification handler.
///
/// `source_db` is the content-addressed [`SourceDatabase`]: file text, cached parse trees, file
/// DBs, and the per-file [`crate::semantic::SymbolTable`]. Includes are read from disk once and then
/// served from cache across keystrokes; the symbol table builds lazily on first query and is
/// invalidated by the change-log drop on edit.
pub struct GlobalState {
    pub source_db: ContentCacheDb,
}

/// A resolved cursor location: the document, its parse, its offset bookkeeping, and the byte offset
/// of the cursor. The shared prologue of every read-handler (goto-definition, rename, references,
/// hover, completion) — each opens `id_for_url → ast → file_db → offset` identically, so it lives
/// here once. Handlers use the fields they need (completion uses `id`+`offset`; goto/rename/
/// references also use `ast`).
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
    /// Construct with the workspace `roots` used to confine `include` resolution. The roots are
    /// canonicalized and stored on the source db; an empty list means no include is ever loaded
    /// (fail-closed against path traversal).
    pub fn new(roots: Vec<PathBuf>) -> Self {
        let mut source_db = ContentCacheDb::new();
        source_db.set_workspace_roots(roots);
        Self { source_db }
    }

    /// Resolve `(uri, position)` to a [`CursorContext`] — the shared open-document prologue.
    /// Returns `None` for an unknown file or one that fails to parse.
    pub(crate) fn cursor_context(
        &self,
        uri: &Url,
        position: lsp_types::Position,
    ) -> Option<CursorContext> {
        let id = self.source_db.id_for_url(uri)?;
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

    /// Dispatch an LSP request to its handler, selected by method name.
    ///
    /// Each arm deserializes the params, delegates to `handler::<feature>::handle`, and wraps the
    /// typed result into a success `Response`. Adding a new request = one new arm here + a handler
    /// module + a capability entry. Returns `Ok(None)` for methods the server does not handle.
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
            _ => Ok(None),
        }
    }

    /// Dispatch an LSP notification by method name. Only document open/change are handled; any
    /// other notification is ignored.
    pub fn handle_notification(&mut self, not: Notification) -> Result<()> {
        match not.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams = serde_json::from_value(not.params)?;
                self.handle_update(&TextDocument::from(params))?;
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams = serde_json::from_value(not.params)?;
                self.handle_update(&TextDocument::from(params))?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Resolve the token's definition(s) to LSP [`Location`]s — the goto-definition shaper. An
    /// include-path string routes to [`jump_to_lib`]; any other token resolves (possibly
    /// cross-file) via [`Self::resolve_use`] and each result is tagged with its owning file's URL.
    pub fn lookup_definition(&self, file_db: &FileDB, token: &SyntaxToken) -> Vec<Location> {
        if token.kind() == TokenKind::CircomString {
            return jump_to_lib(file_db, token, self.source_db.vfs());
        }
        self.resolve_use(file_db, token)
            .into_iter()
            .map(|(id, s)| Location::new(self.source_db.file_db(id).file_path.clone(), s.def_range))
            .collect()
    }

    /// The [`FileId`]s of every include loaded for `origin` (resolved via the source db's confined
    /// include loader). Shared by [`Self::resolve_use`] (cross-file component resolution) and
    /// [`Self::resolve_template_file`] (member completion) so the include walk lives in one place.
    fn loaded_includes(&self, origin: &FileDB) -> Vec<FileId> {
        let Some(ast) = self.source_db.ast(origin.file_id) else {
            return Vec::new();
        };
        ast.libs()
            .into_iter()
            .filter_map(|inc| inc.lib())
            .filter_map(|path| {
                self.source_db
                    .id_for_include(&origin.file_path, &path.value())
            })
            .collect()
    }

    /// Resolve `token` to the declaration(s) it refers to, file-tagged. In-file first; then, for a
    /// component declaration/call, each loaded include's top-level by name (cross-file resolution is
    /// file-scope only — template/function names; a signal/var/param in a lib is only reachable via
    /// member access, a separate problem). The shared resolution core for goto-definition, rename,
    /// and references.
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

        // A component declaration/call also resolves to template/function defs in loaded includes.
        let is_component_use = token_ancestors(token)
            .any(|n| AstComponentDecl::can_cast(n.kind()) || AstComponentCall::can_cast(n.kind()));
        if is_component_use {
            let name = token.text();
            for lib_id in self.loaded_includes(origin) {
                let lib_table = self.source_db.symbol_table(lib_id);
                for sym in lib_table.lookup_top_level(name) {
                    out.push((
                        lib_id,
                        ResolvedSymbol {
                            kind: sym.kind,
                            name: sym.name.clone(),
                            def_range: sym.def_range,
                            decl_range: sym.decl_range,
                        },
                    ));
                }
            }
        }
        out
    }

    /// The file that defines the top-level unit `name` (template/function/bus): `origin` itself if
    /// it declares `name`, else the first loaded include that does. Used by member completion to
    /// locate a component's instantiated template (which may live in an included library).
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

    /// Every occurrence of `target` **in its defining file** (`target.0`), as tokens. Rename and
    /// references are in-file by design: each file's `SymbolTable` indexes only that file's own
    /// declarations, so a token resolves unambiguously within its file. Cross-file rename (a
    /// top-level symbol's usages in `include`d files, or renaming from a cross-file usage) needs a
    /// workspace symbol graph and is a follow-up — *not* name/`def_range` matching across files,
    /// which would both miss genuine cross-file usages and risk colliding when two files define a
    /// same-named symbol at the same line:column.
    pub(crate) fn find_occurrences(
        &self,
        target: &(FileId, ResolvedSymbol),
    ) -> Vec<syntax::syntax_node::SyntaxToken> {
        let (def_file, sym) = target;
        let Some(ast) = self.source_db.ast(*def_file) else {
            return Vec::new();
        };
        let table = self.source_db.symbol_table(*def_file);
        resolver::occurrences_in(ast.syntax(), &table, sym)
    }

    /// Register an updated document: set its text (dropping its derived caches) and load every
    /// `include` once so the libraries are in the VFS for cross-file resolution.
    ///
    /// There is no eager semantic index anymore — the symbol table builds lazily on the first
    /// query and is invalidated by the change-log cache drop in [`SourceDatabase`]. A no-op update
    /// (the client resent identical text) short-circuits before the include walk. Non-`file:` URIs
    /// (untitled/git docs), missing parent dirs, and unreadable includes are skipped rather than
    /// crashing the server.
    pub fn handle_update(&mut self, text_document: &TextDocument) -> Result<()> {
        let Some((id, changed)) = self
            .source_db
            .set_document(&text_document.uri, text_document.text.clone())
        else {
            // Non-`file:` scheme (untitled/git) — nothing to index.
            return Ok(());
        };

        // Identical text → nothing changed, so the parse cache and symbol table are still valid.
        if !changed {
            return Ok(());
        }

        // Includes: read from disk once, then serve from cache on subsequent keystrokes. The
        // symbol table for the main file (and its libs) builds lazily on first query.
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

/// Deserialize a request's params, run its handler against `state`, and wrap the typed result into
/// a success `Response`. Kept generic so [`GlobalState::handle_request`] stays a flat
/// one-line-per-feature dispatch table — adding a request is one arm + one handler module.
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

    /// A `TextDocument` built straight from a URI + text (fields are private, but this test module
    /// is inside `global_state`).
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

    /// Regression test for the cache win: editing the main file must never re-read or re-parse an
    /// unchanged `include`. We observe `parse_count` for the library — it must stay at 1 across a
    /// `didChange` of the main file. (Indexing is now lazy: the lib is parsed the first time a query
    /// needs it, not eagerly on open, so this triggers that parse with an explicit `ast` query.)
    #[test]
    fn include_parsed_once_across_keystrokes_test() {
        let main_uri = fixture_uri("with_include/main.circom");
        let lib_uri = fixture_uri("with_include/lib.circom");
        let src = std::fs::read_to_string(main_uri.to_file_path().unwrap()).unwrap();

        // The include must resolve inside a workspace root (path-traversal confinement), so seed
        // the state with the fixture's directory as the root.
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let root = Path::new(&crate_path)
            .join("src/test_files/handler/with_include")
            .canonicalize()
            .unwrap();
        let mut state = GlobalState::new(vec![root]);

        // First open: registers main + loads the lib text into the VFS (no parse yet — indexing is
        // lazy, so the lib isn't parsed until a query needs it).
        state.handle_update(&doc(&main_uri, src.clone())).unwrap();
        let lib_id = state
            .source_db
            .id_for_url(&lib_uri)
            .expect("include should be loaded on first open");

        // Simulate the first query that touches the lib (e.g. a cross-file goto-def): it parses once.
        let _ = state.source_db.ast(lib_id);
        assert_eq!(
            state.source_db.parse_count(lib_id),
            1,
            "lib parsed exactly once on first query"
        );

        // Second didChange of the main file (text differs) — the lib is unchanged, so it must be a
        // cache hit: not re-read (load_include short-circuits) and not re-parsed.
        let src2 = format!("{src}\n// a trailing edit\n");
        state.handle_update(&doc(&main_uri, src2)).unwrap();
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
            .handle_update(&doc(&untitled, "pragma circom 2.0.0;".to_string()))
            .unwrap();
        assert!(state.source_db.id_for_url(&untitled).is_none());
    }
}
