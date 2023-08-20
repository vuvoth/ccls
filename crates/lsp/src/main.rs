use std::fmt::format;

use dashmap::DashMap;
use log::debug;
use parser::grammar::entry::Scope;
use parser::node::Tree;
use parser::parser::Parser;
use parser::token_kind::TokenKind;
use parser::Lexer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::DidChangeTextDocument;
use tower_lsp::lsp_types::request::GotoDefinition;
use tower_lsp::lsp_types::{
    CompletionOptions, DidChangeTextDocumentParams, DidOpenTextDocumentParams,
    GotoDefinitionParams, GotoDefinitionResponse, HoverProviderCapability, InitializedParams,
    OneOf, TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::LspService;
use tower_lsp::{
    lsp_types::{InitializeParams, InitializeResult, MessageType, ServerCapabilities},
    Client, LanguageServer, Server,
};

mod jump_to_definition;

use vfs::{FilePath, VirtualFile};

#[derive(Debug)]
struct Backend {
    client: Client,
    parse_map: DashMap<String, Tree>,
}

#[derive(Debug, Clone)]
struct TextDocumentItem {
    uri: Url,
    text: String,
    version: i32,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
        debug!("initialized");
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, format!("{:?}", params))
            .await;
        self.on_change(
            &TextDocumentItem {
                uri: params.text_document.uri,
                version: params.text_document.version,
                text: params.text_document.text.clone(),
            },
            params.text_document.text.clone(),
        )
        .await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "This is info")
            .await;
        self.client
            .log_message(MessageType::INFO, format!("{:?}", params))
            .await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        Ok(None)
    }
}

impl Backend {
    async fn on_change(&self, text_document: &TextDocumentItem, text: String) {
        let mut parser = Parser::new(&text);
        parser.parse(Scope::CircomProgram);
        let cst = parser.build_tree();
        self.parse_map.insert(text_document.uri.to_string(), cst);
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    debug!("starting up");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        parse_map: DashMap::new(),
    })
    .finish();

    debug!("built service and created backend");

    Server::new(stdin, stdout, socket).serve(service).await;
}
