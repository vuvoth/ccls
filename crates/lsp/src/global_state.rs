use anyhow::Result;
use lsp_server::{Notification, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification as _};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest, References,
    Rename, Request as _,
};
use lsp_types::{DidChangeTextDocumentParams, DidOpenTextDocumentParams, Location, Url};
use parser::token_kind::TokenKind;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::syntax_node::SyntaxToken;

use crate::database::{FileDB, SemanticDB};
use crate::handler;
use crate::handler::goto_definition::{lookup_definition, lookup_node_wrap_token};
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
        Self {
            text: value.content_changes[0].text.to_string(),
            uri: value.text_document.uri,
        }
    }
}

/// Server-wide state shared by every request/notification handler.
///
/// - `source_db` — the content-addressed [`SourceDatabase`]: file text, cached parse trees, file
///   DBs, and (Phase B) symbol tables. Replaces the per-file `ast_map`/`file_map` maps; includes
///   are read from disk once and then served from cache across keystrokes.
/// - `db` — the legacy semantic index (template/function/signal/variable/component info). Kept for
///   goto-definition until Phase B replaces it with the symbol table.
pub struct GlobalState {
    pub source_db: ContentCacheDb,
    pub db: SemanticDB,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            source_db: ContentCacheDb::new(),
            db: SemanticDB::new(),
        }
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
        root: &FileDB,
        ast: &AstCircomProgram,
        token: &SyntaxToken,
    ) -> Vec<Location> {
        let semantic_data = self.db.semantic.get(&root.file_id).unwrap();
        let mut result = lookup_definition(root, ast, semantic_data, token);

        // A string literal resolves within the current file only (the include path itself).
        if token.kind() == TokenKind::CircomString {
            return result;
        }

        // For a component declaration/call, also search the libraries it may instantiate.
        let parent = root.get_path();
        let is_component_use = lookup_node_wrap_token(TokenKind::ComponentDecl, token).is_some()
            || lookup_node_wrap_token(TokenKind::ComponentCall, token).is_some();
        if is_component_use {
            for lib in ast.libs() {
                let Some(lib_abs) = lib.lib() else { continue };
                let Some(parent_dir) = parent.parent() else {
                    continue;
                };
                let lib_path = parent_dir.join(lib_abs.value());
                let Ok(lib_url) = Url::from_file_path(&lib_path) else {
                    continue;
                };

                // Resolve through the cache: the lib was loaded once in `handle_update`.
                let Some(lib_id) = self.source_db.id_for_url(&lib_url) else {
                    continue;
                };
                let Some(ast_lib) = self.source_db.ast(lib_id) else {
                    continue;
                };
                let file_lib = self.source_db.file_db(lib_id);
                let Some(semantic_lib) = self.db.semantic.get(&lib_id) else {
                    continue;
                };
                result.extend(lookup_definition(&file_lib, &ast_lib, semantic_lib, token));
            }
        }

        result
    }

    /// Index an updated document into the caches: register its text (dropping its derived caches) +
    /// load every `include` once, then rebuild the semantic index for it and its includes.
    ///
    /// Non-`file:` URIs (untitled/git docs), missing parent dirs, and unreadable includes are
    /// skipped rather than crashing the server.
    pub fn handle_update(&mut self, text_document: &TextDocument) -> Result<()> {
        let Some(id) = self
            .source_db
            .set_document(&text_document.uri, text_document.text.clone())
        else {
            // Non-`file:` scheme (untitled/git) — nothing to index.
            return Ok(());
        };

        // Rebuild the semantic index for the main file. The parse itself comes from the cache.
        if let Some(ast) = self.source_db.ast(id) {
            let file_db = self.source_db.file_db(id);
            self.db.semantic.remove(&id);
            self.db.circom_program_semantic(&file_db, &ast);

            // Includes: read from disk once, then serve from cache on subsequent keystrokes.
            for lib in ast.libs() {
                let Some(lib_abs) = lib.lib() else { continue };
                let Some(lib_id) = self
                    .source_db
                    .load_include(&text_document.uri, &lib_abs.value())
                else {
                    continue;
                };
                if let Some(lib_ast) = self.source_db.ast(lib_id) {
                    let lib_file = self.source_db.file_db(lib_id);
                    self.db.semantic.remove(&lib_id);
                    self.db.circom_program_semantic(&lib_file, &lib_ast);
                }
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

    /// Regression test for the Phase A win: editing the main file must never re-read or re-parse an
    /// unchanged `include`. We observe `parse_count` for the library — it must stay at 1 across two
    /// `didChange`s of the main file.
    #[test]
    fn include_parsed_once_across_keystrokes_test() {
        let main_uri = fixture_uri("with_include/main.circom");
        let lib_uri = fixture_uri("with_include/lib.circom");
        let src = std::fs::read_to_string(main_uri.to_file_path().unwrap()).unwrap();

        let mut state = GlobalState::new();

        // First open: main + lib each parse once.
        state.handle_update(&doc(&main_uri, src.clone())).unwrap();
        let lib_id = state
            .source_db
            .id_for_url(&lib_uri)
            .expect("include should be loaded on first open");
        assert_eq!(
            state.source_db.parse_count(lib_id),
            1,
            "lib parsed exactly once on first load"
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
        let mut state = GlobalState::new();
        let untitled = Url::parse("untitled:Untitled-1").unwrap();
        // Must not panic, and must not register the document.
        state
            .handle_update(&doc(&untitled, "pragma circom 2.0.0;".to_string()))
            .unwrap();
        assert!(state.source_db.id_for_url(&untitled).is_none());
    }
}
