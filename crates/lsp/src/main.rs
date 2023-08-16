use std::fmt::format;

use dashmap::DashMap;
use log::debug;
use parser::node::Tree;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::DidChangeTextDocument;
use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializedParams, OneOf,
    TextDocumentSyncCapability, TextDocumentSyncKind, CompletionOptions, HoverProviderCapability,
};
use tower_lsp::LspService;
use tower_lsp::{
    lsp_types::{InitializeParams, InitializeResult, MessageType, ServerCapabilities},
    Client, LanguageServer, Server,
};
#[derive(Debug)]
struct Backend {
    client: Client,
    parse_map: DashMap<String, Tree<'static>>,
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
                ..ServerCapabilities::default()
            },
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn initialized(&self, _: InitializedParams) {
        debug!("initialized");
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn did_open(&self, param: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, format!("{:?}", param))
            .await;
    }

    async fn did_change(&self, param: DidChangeTextDocumentParams) {
        self.client.log_message(MessageType::INFO, "This is infom").await;
        self.client
            .log_message(MessageType::INFO, format!("{:?}", param))
            .await
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
