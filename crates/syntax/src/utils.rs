use lsp_types::{Position, Range};
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


mod tests {
    use lsp_types::Position;
    use parser::{ast::{AstNode, CircomProgramAST}, parser::Parser, syntax_node::SyntaxNode, utils::FileUtils};

    use super::{lookup_definition, lookup_token_at_postion};

    #[test]
    fn parse_un_complete_program() {
        let source: String = 
r#"
pragma circom 2.0.0;

template X() {
   component x = Multiplier2();
   component y = X();
   component y = Multiplier2();
   component z = Multiplier2();
      
}
//
template M() {
   component h = X();
   component k = Multiplier2();   
}

template Multiplier2 () {  
   template m = M();
   // hello world
   signal input a;  
   signal input b;  
      signal output c;  
   component y = X();
   
   component e = Y();
   component z = Y();
   component h = Y();
   signal output d;
   c <== a * b; 
}

template Y() {
   component y = X();
   component a = X();
   
}

"#
        .to_string();

        let green_node = Parser::parse_circom(&source);
        let syntax_node = SyntaxNode::new_root(green_node.clone());
        let file = FileUtils::create(&source);

        if let Some(program_ast) = CircomProgramAST::cast(syntax_node) {
            let position = Position::new(13, 24);
            if let Some(token) = lookup_token_at_postion(&file, &program_ast, position) {
                if let Some(range) = lookup_definition(&file, &program_ast, token) {
                    println!("{range:?}");
                }
            } else {
                println!("None");
            }
        }
    }
}