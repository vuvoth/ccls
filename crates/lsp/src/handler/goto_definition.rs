use lsp_types::{Location, Range, Url};
use parser::token_kind::TokenKind;
use rowan::{ast::AstNode, TextSize};

use syntax::abstract_syntax_tree::{AstCircomProgram, AstInclude};
use syntax::syntax_node::{SyntaxNode, SyntaxToken};

use crate::file_db::FileDB;
use crate::global_state::GlobalState;
use crate::source_db::SourceDatabase;

use anyhow::Result;
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse};

/// Entry point for the `textDocument/definition` request.
///
/// Resolves the token under the cursor, then runs the (possibly cross-file) definition lookup. If
/// the file is unknown to the server or no token is under the cursor, returns `None`.
pub fn handle(
    state: &GlobalState,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // Resolve URI → FileId → (ast, file_db) through the content cache.
    let Some(id) = state.source_db.id_for_url(&uri) else {
        return Ok(None);
    };
    let Some(ast) = state.source_db.ast(id) else {
        return Ok(None);
    };
    let file_db = state.source_db.file_db(id);

    let offset = file_db.offset(position);
    let locations = match token_at_offset(&ast, offset) {
        Some(token) => state.lookup_definition(&file_db, &ast, &token),
        None => Vec::new(),
    };
    Ok(Some(GotoDefinitionResponse::Array(locations)))
}

/// The first `Identifier` or `CircomString` token covering `offset`, or `None`. A thin wrapper
/// over [`rowan::SyntaxNode::token_at_offset`] that picks a semantically meaningful token — every
/// other token kind (whitespace, punctuation, keywords) has no definition to jump to.
pub fn token_at_offset(ast: &AstCircomProgram, offset: TextSize) -> Option<SyntaxToken> {
    ast.syntax().token_at_offset(offset).find_map(|token| {
        let kind = token.kind();
        if kind == TokenKind::Identifier || kind == TokenKind::CircomString {
            Some(token)
        } else {
            None
        }
    })
}

/// The token's wrapping nodes: its parent followed by all of that parent's ancestors. Mirrors
/// `parent_ancestors()`, which is absent on `SyntaxToken` in this rowan version.
pub fn token_ancestors(token: &SyntaxToken) -> impl Iterator<Item = SyntaxNode> {
    token
        .parent()
        .into_iter()
        .flat_map(|p| p.ancestors().collect::<Vec<_>>())
}

// If `token` is an include path (`include "lib.circom";`), jump to that library file's URL.
// Routed here (never the resolver) because the resolver only handles `Identifier` tokens — a
// `CircomString` carries a path, not a symbol name.
pub fn jump_to_lib(file_db: &FileDB, token: &SyntaxToken) -> Vec<Location> {
    let Some(include_stmt) = token_ancestors(token).find_map(AstInclude::cast) else {
        return Vec::new();
    };
    let Some(include_path) = include_stmt.lib() else {
        return Vec::new();
    };
    let path = file_db.get_path();
    let Some(parent_dir) = path.parent() else {
        return Vec::new();
    };
    let lib_path = parent_dir.join(include_path.value());
    // Absolutize so the returned location URL is canonical and matches the FileId the source db
    // interns for the same include (otherwise a `"../lib.circom"` include resolves to a
    // non-canonical URL like `/a/b/../lib.circom` that won't match the interned `/a/lib.circom`).
    let Some(vpath) = vfs::VfsPath::from_abs_path(&lib_path) else {
        return Vec::new();
    };
    let Ok(lib_url) = Url::from_file_path(vpath.as_path()) else {
        return Vec::new();
    };
    vec![Location::new(lib_url, Range::default())]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::Url;
    use rowan::ast::AstNode;
    use syntax::{
        abstract_syntax_tree::{AstCircomProgram, AstInputSignalDecl, AstTemplateDef},
        syntax::syntax_tree,
    };

    use crate::file_db::FileDB;

    use super::token_at_offset;

    fn get_source_from_path(file_path: &str) -> String {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let full_path = format!("{}{}", crate_path, file_path);
        std::fs::read_to_string(&full_path).expect(&full_path)
    }

    /// Replaces the legacy `lookup_node_wrap_token(TokenKind::TemplateDef, &token)` call with typed
    /// ancestor navigation. The resolved node (and hence the insta snapshot) is identical — only the
    /// navigation primitive changes from a kind-string walk to a typed cast.
    #[test]
    fn goto_decl_test() {
        let file_path = "/src/test_files/handler/templates.circom";
        let source = get_source_from_path(file_path);
        let file_db = FileDB::new(
            vfs::FileId(0),
            &source,
            Url::from_file_path(Path::new("/tmp")).unwrap(),
        );

        let syntax_node = syntax_tree(&source);

        if let Some(program_ast) = AstCircomProgram::cast(syntax_node) {
            let inputs = program_ast.template_list()[0]
                .body()
                .unwrap()
                .statement_list()
                .unwrap()
                .find_children::<AstInputSignalDecl>();
            let signal_name = inputs[0].signal_identifier().unwrap().name().unwrap();

            let signal_offset = signal_name.syntax().text_range().start();

            if let Some(token) = token_at_offset(
                &program_ast,
                file_db.offset(file_db.position(signal_offset)),
            ) {
                let template_def = super::token_ancestors(&token).find_map(AstTemplateDef::cast);

                let node_text = match template_def {
                    None => "None".to_string(),
                    Some(def) => format!("{}", def.syntax()),
                };

                insta::assert_snapshot!("test_lookup_node_wrap_token", node_text);
            }
        }
    }

    #[test]
    fn url_test() {
        let url = Url::from_file_path(Path::new("/hello/abc.tx"));
        let binding = url.unwrap();
        let path = binding.path();
        let parent = Path::new(path).parent().unwrap().to_str().unwrap();

        assert_eq!("/hello", parent);
    }
}
