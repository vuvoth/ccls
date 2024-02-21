use std::{fs, path::PathBuf};

use crate::database::{FileDB, SemanticDB, SemanticData, SemanticInfo, TokenId};
use anyhow::Result;
use dashmap::DashMap;
use lsp_server::{RequestId, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, Location, Url,
};

use parser::token_kind::TokenKind;
use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::{self, AstCircomProgram};
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
        semantic_data: &SemanticData,
        token: &SyntaxToken,
    ) -> Vec<Location> {
        let mut result = lookup_definition(root, ast, semantic_data,  token);

        let p = root.get_path();

        for lib in ast.libs() {
            let lib_abs_path = PathBuf::from(lib.lib().unwrap().value());
            let lib_path = p.parent().unwrap().join(lib_abs_path).clone();
            let url = Url::from_file_path(lib_path.clone()).unwrap();
            if let Ok(src) = fs::read_to_string(lib_path) {
                let text_doc = TextDocument {
                    text: src,
                    uri: url.clone(),
                };

                let file = &FileDB::create(&text_doc.text, url.clone());

                let syntax = SyntaxTreeBuilder::syntax_tree(&text_doc.text);

                if let Some(lib_ast) = AstCircomProgram::cast(syntax) {
                    let lib_id = file.file_id;
                    let lib_semantic = self.db.semantic.get(&lib_id).unwrap();
                    let ans = lookup_definition(file, &lib_ast, &lib_semantic, token);
                    result.extend(ans);
                }
            }
        }

        result
    }
    pub fn goto_definition_handler(&self, id: RequestId, params: GotoDefinitionParams) -> Response {
        let uri = params.text_document_position_params.text_document.uri;

        let ast = self.ast_map.get(&uri.to_string()).unwrap();
        let file = self.file_map.get(&uri.to_string()).unwrap();
        eprintln!("goto {:?}", file.file_id);
        eprintln!("{:?}", self.db.semantic);
        let semantic_data = self.db.semantic.get(&file.file_id).unwrap();
        let mut locations = Vec::new();
        if let Some(token) =
            lookup_token_at_postion(&file, &ast, params.text_document_position_params.position)
        {
            locations = self.lookup_definition(&file, &ast, &semantic_data, &token);
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

        eprintln!("{}", AstCircomProgram::can_cast(TokenKind::CircomProgram));
        if let Some(ast) = AstCircomProgram::cast(syntax) {
            eprintln!("{}", url.to_string());
            eprintln!("{:?}", file_id);
            self.db.semantic.remove(&file_id);
            self.db.circom_program_semantic(&file_db, &ast);
            self.ast_map.insert(url.to_string(), ast);
        }

        self.file_map.insert(url.to_string(), file_db);

        Ok(())
    }
}
