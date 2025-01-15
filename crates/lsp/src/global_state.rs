use std::{fs, path::PathBuf};

use crate::{
    database::{FileDB, SemanticDB},
    handler::goto_definition::lookup_node_wrap_token,
};
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

/// state of all (circom) source file
pub struct GlobalState {
    /// key: file id (from file url) - value: ast of its content (source code)
    pub ast_map: DashMap<String, AstCircomProgram>,

    /// key: file id (from file url) - value: file content (+ end lines)
    pub file_map: DashMap<String, FileDB>,

    /// key: file id (from file url) - value: database (template in4, function in4...)
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
        // look up token in current file
        let semantic_data = self.db.semantic.get(&root.file_id).unwrap();
        let mut result = lookup_definition(root, ast, semantic_data, token);

        if token.kind() == TokenKind::CircomString {
            return result;
        }

        
        // if can not find that token in current file,
        // and if token in a component call / declaration
        // continue looking up in libs
        let p = root.get_path();

        if lookup_node_wrap_token(TokenKind::ComponentDecl, token).is_some()
            || lookup_node_wrap_token(TokenKind::ComponentCall, token).is_some()
        {
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
        }

        result
    }

    pub fn goto_definition_handler(&self, id: RequestId, params: GotoDefinitionParams) -> Response {
        // path to the file that contains the element we want to get definition
        // eg: file:///mnt/d/language-server/test-circom/program2.circom
        let uri = params.text_document_position_params.text_document.uri;
        
        // reference to the abtract syntax tree for the file from that uri
        // eg: Ref { k: 0x56136e3ce100, v: 0x56136e3ce118 }
        // ast.key() = "file:///mnt/d/language-server/test-circom/program2.circom"
        // ast.value() = AstCircomProgram { syntax: CircomProgram@0..2707 }
        let ast = self.ast_map.get(&uri.to_string()).unwrap();
        
        // information of the file contains the element we want to get definition
        // eg: Ref { k: 0x56136e3bf5a0, v: 0x56136e3bf5b8 }
        // file.key() = "file:///mnt/d/language-server/test-circom/program2.circom"
        // file.value() = 
        // FileDB { 
        //     file_id: FileId(17547606022754654883), 
        //     file_path: Url { 
        //         scheme: "file", 
        //         cannot_be_a_base: false, 
        //         username: "", 
        //         password: None, 
        //         host: None, 
        //         port: None, 
        //         path: "/mnt/d/language-server/test-circom/program2.circom", 
        //         query: None, 
        //         fragment: None 
        //     }, 
        //     end_line_vec: [2, 44, ..., 2701] 
        // }
        let file = self.file_map.get(&uri.to_string()).unwrap();
        
        let mut locations = Vec::new();

        // extract token from ast at position (file, params position)
        // eg: token = Identifier@2205..2207 "e2"
        if let Some(token) =
            lookup_token_at_postion(&file, &ast, params.text_document_position_params.position)
        {
            locations = self.lookup_definition(&file, &ast, &token);
            // locations of declarations of that element
            // it may returns more than 1 location if exist same name declarations
            // eg: 
            // [
            //     Location { 
            //         uri: Url { 
            //             scheme: "file", 
            //             cannot_be_a_base: false, 
            //             username: "", 
            //             password: None, 
            //             host: None, 
            //             port: None, 
            //             path: "/mnt/d/language-server/test-circom/program2.circom", 
            //             query: None, 
            //             fragment: None 
            //         }, 
            //         range: Range { 
            //             start: Position { line: 75, character: 8 }, 
            //             end: Position { line: 75, character: 14 } 
            //         } 
            //     }
            // ]
        };

        let result: Option<GotoDefinitionResponse> = Some(GotoDefinitionResponse::Array(locations));

        let result = serde_json::to_value(result).unwrap();
        // serialize result into JSON format
        // eg:
        // Array [
        //     Object {
        //         "range": Object {
        //             "end": Object {
        //                 "character": Number(14), 
        //                 "line": Number(75)
        //             }, 
        //             "start": Object {
        //                 "character": Number(8), 
        //                 "line": Number(75)
        //             }
        //         }, 
        //         "uri": String("file:///mnt/d/language-server/test-circom/program2.circom")
        //     }
        // ]

        Response {
            id,
            result: Some(result),
            error: None,
        }
    }

    /// update a file of (circom) source code
    /// parse new code --> syntax tree
    /// remove old data of that file in semantic database
    /// add new data (circom_program_semantic) + related libs into database
    /// update corresponding file-map and ast-map in global-state
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
