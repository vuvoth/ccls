use std::{fs, path::PathBuf};

use crate::database::{FileDB, SemanticDB};
use anyhow::Result;
use dashmap::DashMap;
use lsp_server::{RequestId, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, Location, Url,
};

use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::syntax::SyntaxTreeBuilder;
use syntax::syntax_node::SyntaxToken;

use crate::handler::goto_definition::{lookup_definition, lookup_token_at_postion};

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

pub struct GlobalState {
    pub ast_map: DashMap<String, AstCircomProgram>,
    pub file_map: DashMap<String, FileDB>,
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

    pub fn lookup_definition(
        &self,
        root: &FileDB,
        ast: &AstCircomProgram,
        token: &SyntaxToken,
    ) -> Vec<Location> {
        let semantic_data = self.db.semantic.get(&root.file_id).unwrap();
        let mut result = lookup_definition(root, ast, semantic_data, token);

        if token.kind() == TokenKind::CircomString {
            return result;
        }

        let p = root.get_path();

        for lib in ast.libs() {
            let lib_abs_path = PathBuf::from(lib.lib().unwrap().value());
            let lib_path = p.parent().unwrap().join(lib_abs_path).clone();
            let lib_url = Url::from_file_path(lib_path.clone()).unwrap();

            if let Some(file_lib) = self.file_map.get(&lib_url.to_string()) {
                let ast_lib = self.ast_map.get(&lib_url.to_string()).unwrap();
                if let Some(semantic_data_lib) = self.db.semantic.get(&file_lib.file_id) {
                    let lib_result =
                        lookup_definition(&file_lib, &ast_lib, semantic_data_lib, token);
                    result.extend(lib_result);
                }
            }
        }
        result
    }

    pub fn goto_definition_handler(&self, id: RequestId, params: GotoDefinitionParams) -> Response {
        let uri = params.text_document_position_params.text_document.uri;

        let ast = self.ast_map.get(&uri.to_string()).unwrap();
        let file = self.file_map.get(&uri.to_string()).unwrap();

        let mut locations = Vec::new();

        if let Some(token) =
            lookup_token_at_postion(&file, &ast, params.text_document_position_params.position)
        {
            locations = self.lookup_definition(&file, &ast, &token);
        };

        let result: Option<GotoDefinitionResponse> = Some(GotoDefinitionResponse::Array(locations));

        let result = serde_json::to_value(result).unwrap();

        Response {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn handle_update(&mut self, text_document: &TextDocument) -> Result<()> {
        let text = &text_document.text;
        let url = &text_document.uri.to_string();

        let syntax = SyntaxTreeBuilder::syntax_tree(text);
        let file_db = FileDB::create(text, text_document.uri.clone());
        let file_id = file_db.file_id;

        let p: PathBuf = file_db.get_path();
        if let Some(ast) = AstCircomProgram::cast(syntax) {
            self.db.semantic.remove(&file_id);
            self.db.circom_program_semantic(&file_db, &ast);

            for lib in ast.libs() {
                if let Some(lib_abs_path) = lib.lib() {
                    let lib_path = p.parent().unwrap().join(lib_abs_path.value()).clone();
                    let lib_url = Url::from_file_path(lib_path.clone()).unwrap();
                    if let Ok(src) = fs::read_to_string(lib_path) {
                        let text_doc = TextDocument {
                            text: src,
                            uri: lib_url.clone(),
                        };
                        let lib_file = FileDB::create(&text_doc.text, lib_url.clone());
                        let syntax = SyntaxTreeBuilder::syntax_tree(&text_doc.text);

                        if let Some(lib_ast) = AstCircomProgram::cast(syntax) {
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
