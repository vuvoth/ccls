use std::fs;

use anyhow::Result;
use dashmap::DashMap;
use lsp_server::{RequestId, Response};
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, HoverParams, Location, Url};
use parser::{Rule, Token};
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::Program;
use syntax::syntax::SyntaxTreeBuilder;
use syntax::syntax_node::SyntaxKind;

use crate::{
    database::{FileDB, SemanticDB},
    handler::{resolve, resolve_include, token_at},
};

pub struct TextDocument {
    pub text: String,
    pub uri: Url,
}

impl From<lsp_types::DidOpenTextDocumentParams> for TextDocument {
    fn from(p: lsp_types::DidOpenTextDocumentParams) -> Self {
        Self {
            text: p.text_document.text,
            uri: p.text_document.uri,
        }
    }
}

impl From<lsp_types::DidChangeTextDocumentParams> for TextDocument {
    fn from(p: lsp_types::DidChangeTextDocumentParams) -> Self {
        Self {
            text: p
                .content_changes
                .first()
                .map(|c| c.text.clone())
                .unwrap_or_default(),
            uri: p.text_document.uri,
        }
    }
}

pub struct GlobalState {
    programs: DashMap<String, Program>,
    files: DashMap<String, FileDB>,
    db: SemanticDB,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            programs: DashMap::new(),
            files: DashMap::new(),
            db: SemanticDB::default(),
        }
    }

    pub fn goto_definition(&self, id: RequestId, params: GotoDefinitionParams) -> Response {
        let uri = params.text_document_position_params.text_document.uri;
        let key = uri.to_string();

        let Some(program) = self.programs.get(&key) else {
            return self.null_response(id);
        };
        let Some(file) = self.files.get(&key) else {
            return self.null_response(id);
        };

        let locations = self.resolve_definitions(
            &file,
            &program,
            params.text_document_position_params.position,
        );

        Response {
            id,
            result: Some(
                serde_json::to_value(Some(GotoDefinitionResponse::Array(locations))).unwrap(),
            ),
            error: None,
        }
    }

    pub fn hover(&self, id: RequestId, _params: HoverParams) -> Response {
        self.null_response(id)
    }

    pub fn update(&mut self, doc: &TextDocument) -> Result<()> {
        let key = doc.uri.to_string();
        let syntax = SyntaxTreeBuilder::syntax_tree(&doc.text);
        let file = FileDB::new(&doc.text, doc.uri.clone());

        let Some(program) = Program::cast(syntax) else {
            return Ok(());
        };

        self.db.files.remove(&file.id);
        self.db.index_program(&file, &program);

        // Load includes
        for include in program.includes() {
            if let Some(path) = include.path() {
                self.load_include(&file, &path)?;
            }
        }

        self.programs.insert(key.clone(), program);
        self.files.insert(key, file);
        Ok(())
    }

    fn load_include(&mut self, file: &FileDB, path: &str) -> Result<()> {
        let lib_path = file
            .path()
            .parent()
            .ok_or_else(|| anyhow::anyhow!("No parent dir"))?
            .join(path);

        let lib_url =
            Url::from_file_path(&lib_path).map_err(|_| anyhow::anyhow!("Invalid path"))?;
        let key = lib_url.to_string();

        if self.files.contains_key(&key) {
            return Ok(());
        }

        let text = fs::read_to_string(&lib_path)?;
        let lib_file = FileDB::new(&text, lib_url);
        let syntax = SyntaxTreeBuilder::syntax_tree(&text);

        if let Some(program) = Program::cast(syntax) {
            self.db.index_program(&lib_file, &program);
            self.programs.insert(key.clone(), program);
        }

        self.files.insert(key, lib_file);
        Ok(())
    }

    fn resolve_definitions(
        &self,
        file: &FileDB,
        program: &Program,
        pos: lsp_types::Position,
    ) -> Vec<Location> {
        let Some(token) = token_at(file, program, pos) else {
            return Vec::new();
        };

        let Some(semantics) = self.db.get(file.id) else {
            return Vec::new();
        };

        // Handle include paths
        if token.kind() == SyntaxKind::from_token(Token::String) {
            return resolve_include(file, &token);
        }

        let mut results = resolve(file, program, semantics, &token);

        // Cross-file lookup for component/template references
        if is_cross_file_ref(&token) {
            results.extend(self.cross_file_lookup(file, program, &token));
        } else if results.is_empty() && is_identifier_token(&token) {
            // Also try cross-file lookup if local resolution failed for an identifier.
            // This handles inline template calls like: signal x <== Template()([args])
            // where the token is not inside a TemplateCall or ComponentDecl node.
            results.extend(self.cross_file_lookup(file, program, &token));
        }

        results
    }

    fn cross_file_lookup(
        &self,
        file: &FileDB,
        program: &Program,
        token: &syntax::syntax_node::SyntaxToken,
    ) -> Vec<Location> {
        let mut results = Vec::new();
        let path = file.path();
        let Some(parent) = path.parent() else {
            return results;
        };

        for include in program.includes() {
            let Some(path) = include.path() else { continue };
            let lib_path = parent.join(path);

            let Ok(lib_url) = Url::from_file_path(&lib_path) else {
                continue;
            };
            let key = lib_url.to_string();

            let Some(lib_file) = self.files.get(&key) else {
                continue;
            };
            let Some(lib_program) = self.programs.get(&key) else {
                continue;
            };
            let Some(lib_semantics) = self.db.get(lib_file.id) else {
                continue;
            };

            results.extend(resolve(&lib_file, &lib_program, lib_semantics, token));
        }

        results
    }

    fn null_response(&self, id: RequestId) -> Response {
        Response {
            id,
            result: Some(serde_json::Value::Null),
            error: None,
        }
    }
}

fn is_cross_file_ref(token: &syntax::syntax_node::SyntaxToken) -> bool {
    token.parent_ancestors().any(|n| {
        n.kind() == SyntaxKind::from_rule(Rule::ComponentDecl)
            || n.kind() == SyntaxKind::from_rule(Rule::TemplateCall)
    })
}

fn is_identifier_token(token: &syntax::syntax_node::SyntaxToken) -> bool {
    token.kind() == SyntaxKind::from_token(Token::Identifier)
}
