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
use syntax::abstract_syntax_tree::{AstCircomProgram, AstComponentCall, AstComponentDecl};
use syntax::syntax_node::SyntaxToken;

use std::path::PathBuf;

use crate::file_db::FileDB;
use crate::handler;
use crate::handler::goto_definition::{jump_to_lib, token_ancestors};
use crate::resolver;
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

    /// Resolve every declaration of the symbol carried by `token`, searching the current file first
    /// and then any `include`d library when the token sits in a component declaration/call.
    pub fn lookup_definition(
        &self,
        file_db: &FileDB,
        ast: &AstCircomProgram,
        token: &SyntaxToken,
    ) -> Vec<Location> {
        // A string literal resolves to the included library file (the include path is searched in
        // the current file only). Hoisted to the top so the resolver stays pure / URL-agnostic and
        // never sees `CircomString` tokens.
        if token.kind() == TokenKind::CircomString {
            return jump_to_lib(file_db, token, self.source_db.vfs());
        }

        // In-file resolution: the symbol table builds lazily on first query (never an `unwrap` —
        // a failed parse yields an empty table).
        let table = self.source_db.symbol_table(file_db.file_id);
        let file_symbols = resolver::resolve(&table, token);
        let mut locations: Vec<Location> = file_symbols
            .into_iter()
            .map(|s| Location::new(file_db.file_path.clone(), s.def_range))
            .collect();

        // For a component declaration/call, also search the libraries it may instantiate. Cross-file
        // resolution is **file-scope only** (template/function names): a signal/var/param in a lib is
        // only reachable via member-access (e.g. `c.signal`), which is a separate problem and out of
        // scope. Resolution is by name in each lib's own table — never reusing the main-file token's
        // identity (the bug that made the legacy `hash(text)` index structurally return `None` here).
        let is_component_use = token_ancestors(token)
            .any(|n| AstComponentDecl::can_cast(n.kind()) || AstComponentCall::can_cast(n.kind()));
        if is_component_use {
            let name = token.text();
            for include in ast.libs() {
                let Some(include_path) = include.lib() else {
                    continue;
                };
                let Some(lib_id) = self
                    .source_db
                    .id_for_include(&file_db.file_path, &include_path.value())
                else {
                    continue;
                };
                let lib_table = self.source_db.symbol_table(lib_id);
                let lib_file = self.source_db.file_db(lib_id);
                for sym in lib_table.lookup_top_level(name) {
                    locations.push(Location::new(lib_file.file_path.clone(), sym.def_range));
                }
            }
        }

        locations
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
