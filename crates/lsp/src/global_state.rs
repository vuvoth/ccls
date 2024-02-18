use anyhow::Result;
use dashmap::DashMap;
use lsp_server::{RequestId, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, Location, Url,
};
use parser::{
    ast::{AstCircomProgram, AstNode},
    parser::Parser,
    syntax_node::SyntaxNode,
    utils::FileUtils,
};

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
    pub file_map: DashMap<String, FileUtils>,
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            ast_map: DashMap::new(),
            file_map: DashMap::new(),
        }
    }

    pub fn goto_definition_handler(&self, id: RequestId, params: GotoDefinitionParams) -> Response {
        let uri = params.text_document_position_params.text_document.uri;

        let ast = self.ast_map.get(&uri.to_string()).unwrap();
        let file = self.file_map.get(&uri.to_string()).unwrap();

        let mut result = Some(GotoDefinitionResponse::Array(Vec::new()));

        eprintln!("{:?}", params.text_document_position_params.position);
        if let Some(token) =
            lookup_token_at_postion(&file, &ast, params.text_document_position_params.position)
        {
            if let Some(range) = lookup_definition(&file, &ast, token) {
                result = Some(GotoDefinitionResponse::Scalar(Location::new(uri, range)));
            };
        }

        let result = serde_json::to_value(result).unwrap();

        Response {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub(crate) fn handle_update(&mut self, text_document: &TextDocument) -> Result<()> {
        let text = &text_document.text;
        let url = text_document.uri.to_string();

        let green = Parser::parse_circom(text);
        let syntax = SyntaxNode::new_root(green);

        self.ast_map
            .insert(url.clone(), AstCircomProgram::cast(syntax).unwrap());

        self.file_map.insert(url, FileUtils::create(text));
        Ok(())
    }
}
