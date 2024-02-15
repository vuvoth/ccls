use std::fmt::format;
use std::sync::Arc;

use ::parser::node::Tree;
use ::parser::parser::Parser;
use dashmap::DashMap;
use parser::parser;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::DidChangeTextDocument;
use tower_lsp::lsp_types::request::GotoDefinition;
use tower_lsp::lsp_types::{
    CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    GotoDefinitionParams, GotoDefinitionResponse, HoverProviderCapability, InitializedParams,
    Location, OneOf, Position, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::LspService;
use tower_lsp::{
    lsp_types::{InitializeParams, InitializeResult, MessageType, ServerCapabilities},
    Client, LanguageServer, Server,
};

#[derive(Debug)]
struct Backend {
    client: Client,
    parse_map: DashMap<String, Tree>,
}

#[derive(Debug, Clone)]
struct TextDocumentItem<'a> {
    uri: Url,
    text: &'a str,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(MessageType::INFO, format!("WE init {:?}", params))
            .await;
        self.client
            .log_message(MessageType::INFO, "initializing!")
            .await;

        Ok(InitializeResult {
            server_info: None,
            offset_encoding: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, format!("{:?}", params))
            .await;
        self.on_change(&TextDocumentItem {
            uri: params.text_document.uri,
            text: &params.text_document.text,
        })
        .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "This is info change")
            .await;
        self.on_change(&TextDocumentItem {
            uri: params.text_document.uri,
            text: &params.content_changes[0].text,
        })
        .await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = params.text_document_position_params.clone();
        let ast = self
            .parse_map
            .get(&position.text_document.uri.to_string())
            .unwrap();
        let pos = Position {
            line: position.position.line + 1,
            character: position.position.character,
        };

        if let Some(token) = ast.lookup_element_by_range(pos) {
            let ranges = ast.lookup_definition(token);

            let result = ranges
                .iter()
                .map(move |range| Location {
                    uri: position.text_document.uri.clone(),
                    range: *range,
                })
                .collect();

            Ok(Some(GotoDefinitionResponse::Array(result)))
        } else {
            Ok(None)
        }
    }
}

impl Backend {
    async fn on_change(&self, text_document: &TextDocumentItem<'_>) {
        let cst_result = Parser::parse_source(&text_document.text);

        match cst_result {
            Ok(cst) => {
                self.parse_map.insert(text_document.uri.to_string(), cst);
            }
            _ => {
                self.client
                    .log_message(MessageType::INFO, "Somthing wrong")
                    .await;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        parse_map: DashMap::new(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
