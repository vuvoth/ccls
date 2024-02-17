#![allow(clippy::print_stderr)]

use std::collections::HashMap;
use std::error::Error;

use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument};
use lsp_types::{
    request::GotoDefinition, GotoDefinitionResponse, InitializeParams, ServerCapabilities,
};
use lsp_types::{Location, OneOf, TextDocumentSyncCapability, TextDocumentSyncKind};

use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use parser::ast::{AstNode, CircomProgramAST};
use parser::parser::Parser;
use parser::syntax_node::SyntaxNode;
use parser::utils::{FileId, FileUtils};

use crate::handler::goto_definition::{lookup_definition, lookup_token_at_postion};

mod handler;

struct GlobalState {
    pub ast: HashMap<String, CircomProgramAST>,
    pub file_map: HashMap<String, FileUtils>,
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    // Note that  we must have our logging only write out to stderr.
    eprintln!("starting generic LSP server");

    // Create the transport. Includes the stdio (stdin and stdout) versions but this could
    // also be implemented to use sockets or HTTP.
    let (connection, io_threads) = Connection::stdio();

    // Run the server and wait for the two threads to end (typically by trigger LSP Exit event).
    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    })
    .unwrap();

    let initialization_params = match connection.initialize(server_capabilities) {
        Ok(it) => it,
        Err(e) => {
            if e.channel_is_disconnected() {
                io_threads.join()?;
            }
            return Err(e.into());
        }
    };
    main_loop(connection, initialization_params)?;
    io_threads.join()?;

    // Shut down gracefully.
    eprintln!("shutting down server");
    Ok(())
}

fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params).unwrap();

    let mut global_state = GlobalState {
        ast: HashMap::new(),
        file_map: HashMap::new(),
    };

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                match cast::<GotoDefinition>(req) {
                    Ok((id, params)) => {
                        let uri = params.text_document_position_params.text_document.uri;

                        let ast = global_state.ast.get(&uri.to_string()).unwrap();
                        let file = global_state.file_map.get(&uri.to_string()).unwrap();
                        let token = lookup_token_at_postion(
                            &file,
                            ast.syntax(),
                            params.text_document_position_params.position,
                        );

                        let range = lookup_definition(file, &ast, token.unwrap());

                        let result = Some(GotoDefinitionResponse::Scalar(Location::new(
                            uri,
                            range.unwrap(),
                        )));
                        let result = serde_json::to_value(&result).unwrap();
                        let resp = Response {
                            id,
                            result: Some(result),
                            error: None,
                        };
                        connection.sender.send(Message::Response(resp))?;
                        continue;
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                    Err(ExtractError::MethodMismatch(req)) => req,
                };
            }
            Message::Response(resp) => {
                eprintln!("got response: {resp:?}");
            }
            Message::Notification(not) => {
                match cast_notification::<DidOpenTextDocument>(not.clone()) {
                    Ok(params) => {
                        let text = params.text_document.text;
                        let url = params.text_document.uri.to_string();

                        let green = Parser::parse_circom(&text);
                        let syntax = SyntaxNode::new_root(green);

                        global_state
                            .ast
                            .insert(url.clone(), CircomProgramAST::cast(syntax).unwrap());

                        global_state.file_map.insert(url, FileUtils::create(&text));
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                    Err(ExtractError::MethodMismatch(_not)) => (),
                };

                match cast_notification::<DidChangeTextDocument>(not.clone()) {
                    Ok(params) => {
                        let text = &params.content_changes[0].text;
                        let url = params.text_document.uri.to_string();
                        let green = Parser::parse_circom(text);
                        let syntax = SyntaxNode::new_root(green);

                        global_state
                            .ast
                            .insert(url.clone(), CircomProgramAST::cast(syntax).unwrap());

                        global_state.file_map.insert(url, FileUtils::create(&text));
                    }
                    Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                    Err(ExtractError::MethodMismatch(_)) => {}
                }
            }
        }
    }
    Ok(())
}

fn cast<R>(req: Request) -> Result<(RequestId, R::Params), ExtractError<Request>>
where
    R: lsp_types::request::Request,
    R::Params: serde::de::DeserializeOwned,
{
    req.extract(R::METHOD)
}

fn cast_notification<R>(not: Notification) -> Result<R::Params, ExtractError<Notification>>
where
    R: lsp_types::notification::Notification,
    R::Params: serde::de::DeserializeOwned,
{
    not.extract(R::METHOD)
}
