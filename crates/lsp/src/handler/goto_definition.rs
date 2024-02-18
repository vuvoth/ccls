use lsp_server::RequestId;
use lsp_types::{request::GotoDeclarationResponse, GotoDefinitionParams, Position, Range};
use parser::{
    ast::{AstNode, CircomProgramAST},
    syntax_node::SyntaxToken,
    token_kind::TokenKind,
    utils::FileUtils,
};

pub fn lookup_token_at_postion(
    file: &FileUtils,
    ast: &CircomProgramAST,
    position: Position,
) -> Option<SyntaxToken> {
    let off_set = file.off_set(position);
    ast.syntax().token_at_offset(off_set).find_map(|token| {
        let kind = token.kind();

        if kind == TokenKind::Identifier {
            return Some(token);
        }

        None
    })
}

pub fn lookup_definition(
    file: &FileUtils,
    ast: &CircomProgramAST,
    token: SyntaxToken,
) -> Option<Range> {
    let template_list = ast.template_list();

    for template in template_list {
        let template_name = template.template_name().unwrap();
        if template_name.name().text() == token.text() {
            let range = Some(Range {
                start: file.position(template.syntax().text_range().start()),
                end: file.position(template.syntax().text_range().end()),
            });
            return range;
        }
    }
    None
}
