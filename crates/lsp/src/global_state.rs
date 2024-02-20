use std::env;
use std::{fs, path::PathBuf};

use anyhow::Result;
use dashmap::DashMap;
use lsp_server::{RequestId, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, Location, Range, Url,
};

use rowan::ast::AstNode;
use syntax::abstract_syntax_tree::AstCircomProgram;
use syntax::syntax::SyntaxTreeBuilder;
use syntax::syntax_node::SyntaxToken;

use crate::handler::{
    goto_definition::{lookup_definition, lookup_token_at_postion},
    lsp_utils::FileUtils,
};

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
    pub file_map: DashMap<String, FileUtils>,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            ast_map: DashMap::new(),
            file_map: DashMap::new(),
        }
    }

    pub fn lookup_definition(
        &self,
        root: &FileUtils,
        ast: &AstCircomProgram,
        token: &SyntaxToken,
    ) -> Vec<Location> {
        let mut result = lookup_definition(root, ast, token);

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

                let file = &FileUtils::create(&text_doc.text, url.clone());

                let syntax = SyntaxTreeBuilder::syntax_tree(&text_doc.text);

                if let Some(lib_ast) = AstCircomProgram::cast(syntax) {
                    let ans = lookup_definition(file, &lib_ast, token);
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

    pub(crate) fn handle_update(&self, text_document: &TextDocument) -> Result<()> {
        let text = &text_document.text;
        let url = text_document.uri.to_string();

        let syntax = SyntaxTreeBuilder::syntax_tree(&text);

        self.ast_map
            .insert(url.clone(), AstCircomProgram::cast(syntax).unwrap());

        self.file_map
            .insert(url, FileUtils::create(text, text_document.uri.clone()));
        Ok(())
    }
}
