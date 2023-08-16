use dashmap::DashMap;
use log::debug;
use parser::node::Tree;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::InitializedParams;
use tower_lsp::{
    lsp_types::{InitializeParams, InitializeResult, MessageType, ServerCapabilities},
    Client, LanguageServer,
    Server
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
                // definition: Some(GotoCapability::default()),
                // definition_provider: Some(OneOf::Left(true)),
                // references_provider: Some(OneOf::Left(true)),
                // rename_provider: Some(OneOf::Left(true)),
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
