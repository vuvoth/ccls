use anyhow::Result;
use lsp_server::{Notification, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification as _};
use lsp_types::request::{
    Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest,
    PrepareRenameRequest, References, Rename, Request as _,
};
use lsp_types::{DidChangeTextDocumentParams, DidOpenTextDocumentParams, Location, Url};
use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use rowan::TextSize;
use syntax::abstract_syntax_tree::{
    AstCircomProgram, AstComponentCall, AstComponentDecl, AstMainComponent,
};
use syntax::syntax_node::SyntaxToken;

use std::path::PathBuf;

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
        Self { source_db }
    }

    /// Resolve `(uri, position)` to a [`CursorContext`], or `None` if the file is unknown or fails
    /// to parse.
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

    /// Dispatch an LSP notification; only document open/change are handled.
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
            _ => {}
        }
        Ok(())
    }

    /// Goto-definition shaper: an include-path string routes to [`jump_to_lib`]; any other token
    /// resolves via [`Self::resolve_use`] (possibly cross-file), file-tagged.
    pub fn lookup_definition(&self, file_db: &FileDB, token: &SyntaxToken) -> Vec<Location> {
        if token.kind() == TokenKind::CircomString {
            return jump_to_lib(file_db, token, self.source_db.vfs());
        }
        // A component member-access field (`c.x` / `T()(...).x`) resolves via type inference
        // (`resolve_member`), not the flat name resolver (which returns empty for fields by design).
        let resolved = if resolver::component_field(token).is_some() {
            self.resolve_member(file_db, token)
        } else {
            self.resolve_use(file_db, token)
        };
        resolved
            .into_iter()
            .map(|(id, s)| Location::new(self.source_db.file_db(id).file_path.clone(), s.def_range))
            .collect()
    }

    /// The [`FileId`]s of every include loaded for `origin`. Shared by [`Self::resolve_use`] and
    /// [`Self::resolve_template_file`] so the include walk lives in one place.
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
            .map(|s| {
                (
                    template_file,
                    ResolvedSymbol {
                        kind: s.kind,
                        name: s.name.clone(),
                        def_range: s.def_range,
                        decl_range: s.decl_range,
                    },
                )
            })
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
    ) -> Vec<syntax::syntax_node::SyntaxToken> {
        let (def_file, sym) = target;
        let Some(ast) = self.source_db.ast(*def_file) else {
            return Vec::new();
        };
        let table = self.source_db.symbol_table(*def_file);
        resolver::occurrences_in(ast.syntax(), &table, sym)
    }

    /// Register an updated document: set its text (dropping derived caches) and load each `include`
    /// once. No eager index — the symbol table builds lazily and invalidates on edit; a no-op
    /// (identical text) short-circuits; non-`file:` URIs and unreadable includes are skipped, not
    /// crashed. Takes the document by value so `text` moves (not clones) — drops one `String` clone
    /// per keystroke.
    pub fn handle_update(&mut self, text_document: TextDocument) -> Result<()> {
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
}
