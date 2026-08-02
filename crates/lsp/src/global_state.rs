use std::{fs, path::PathBuf};

use anyhow::Result;
use dashmap::DashMap;
use lsp_server::{Notification, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification as _};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest, References,
    Rename, Request as _,
};
use lsp_types::{DidChangeTextDocumentParams, DidOpenTextDocumentParams, Location, Url};
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::syntax::syntax_tree;
use syntax::syntax_node::SyntaxToken;

use crate::database::{FileDB, SemanticDB};
use crate::handler;
use crate::handler::goto_definition::{lookup_definition, lookup_node_wrap_token};

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
/// Three keyed maps (all keyed by the file URI string):
/// - `ast_map`  — the parsed program AST,
/// - `file_map` — the `FileDB` (offset/line bookkeeping),
/// - `db`       — the semantic database (template/function/signal/variable/component info).
pub struct GlobalState {
    /// key: file URI - value: AST of its content.
    pub ast_map: DashMap<String, AstCircomProgram>,
    /// key: file URI - value: file content bookkeeping (offset/line conversion).
    pub file_map: DashMap<String, FileDB>,
    /// Semantic database, keyed internally by `FileId`.
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
            ast_map: DashMap::new(),
            file_map: DashMap::new(),
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
                let lib_path = parent.parent().unwrap().join(lib_abs.value());
                let lib_url = Url::from_file_path(&lib_path).unwrap();

                let Some(file_lib) = self.file_map.get(&lib_url.to_string()) else {
                    continue;
                };
                let Some(ast_lib) = self.ast_map.get(&lib_url.to_string()) else {
                    continue;
                };
                let Some(semantic_lib) = self.db.semantic.get(&file_lib.file_id) else {
                    continue;
                };
                result.extend(lookup_definition(&file_lib, &ast_lib, semantic_lib, token));
            }
        }

        result
    }

    /// Re-parse an updated document: build its syntax tree + semantic data, then do the same for
    /// every `include`d library, and refresh the file/ast maps.
    pub fn handle_update(&mut self, text_document: &TextDocument) -> Result<()> {
        let text = &text_document.text;
        let url = &text_document.uri.to_string();

        let syntax = syntax_tree(text);
        let file_db = FileDB::create(text, text_document.uri.clone());
        let file_id = file_db.file_id;

        let parent: PathBuf = file_db.get_path();
        if let Some(ast) = AstCircomProgram::cast(syntax) {
            self.db.semantic.remove(&file_id);
            self.db.circom_program_semantic(&file_db, &ast);

            for lib in ast.libs() {
                if let Some(lib_abs_path) = lib.lib() {
                    let lib_path = parent.parent().unwrap().join(lib_abs_path.value());
                    let lib_url = Url::from_file_path(&lib_path).unwrap();
                    if let Ok(src) = fs::read_to_string(&lib_path) {
                        let lib_file = FileDB::create(&src, lib_url.clone());
                        if let Some(lib_ast) = AstCircomProgram::cast(syntax_tree(&src)) {
                            self.db.semantic.remove(&lib_file.file_id);
                            self.db.circom_program_semantic(&lib_file, &lib_ast);
                            self.ast_map.insert(lib_url.to_string(), lib_ast);
                        }
                        self.file_map.insert(lib_url.to_string(), lib_file);
                    }
                }
            }
            self.ast_map.insert(url.to_string(), ast);
        }

        self.file_map.insert(url.to_string(), file_db);

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
