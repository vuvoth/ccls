use parser::input::Input;
use parser::output::{Child, Output};
use parser::parser::Parser;
use rowan::{GreenNode, GreenNodeBuilder};

use crate::syntax_node::SyntaxNode;

pub struct SyntaxTreeBuilder<'a> {
    builder: GreenNodeBuilder<'static>,
    input: &'a Input<'a>,
}

impl<'a> SyntaxTreeBuilder<'a> {
    pub fn new(input: &'a Input) -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
            input,
        }
    }
    pub fn build_rec(&mut self, tree: &Output) {
        self.builder.start_node(tree.kind().into());
        for child in tree.children() {
            match child {
                Child::Token(token_id) => {
                    let token_kind = self.input.kind_of(*token_id);
                    let token_value = self.input.token_value(*token_id);
                    self.builder.start_node(token_kind.into());
                    self.builder.token(token_kind.into(), token_value);
                    self.builder.finish_node();
                }
                Child::Tree(child_tree) => self.build_rec(child_tree),
            }
        }

        self.builder.finish_node();
    }

    pub fn build(&mut self, tree: Output) {
        self.build_rec(&tree);
    }

    pub fn finish(self) -> GreenNode {
        self.builder.finish()
    }

    pub fn syntax_tree(source: &str) -> SyntaxNode {
        let input = Input::new(source);

        let output = Parser::parsing(&input);

        let mut builder = SyntaxTreeBuilder::new(&input);
        builder.build(output);
        let green = builder.finish();
        SyntaxNode::new_root(green)
    }
}

#[cfg(test)]
mod tests {
    use parser::token_kind::TokenKind::{self, *};
    use std::hash::{DefaultHasher, Hash, Hasher};

    use rowan::{ast::AstNode, TextRange};

    use crate::{abstract_syntax_tree::AstCircomProgram, test_programs};

    use super::SyntaxTreeBuilder;

    fn generate_expected_token_kind(ast: &AstCircomProgram) {
        let children = ast
            .syntax()
            .first_child()
            .unwrap()
            .siblings(rowan::Direction::Next);

        println!("vec![");
        for child in children {
            println!("{:?},", child.kind());
        }
        println!("];");
    }

    fn generate_expected_token_range(ast: &AstCircomProgram) {
        let children = ast
            .syntax()
            .first_child()
            .unwrap()
            .siblings(rowan::Direction::Next);

        println!("vec![");
        for child in children {
            println!(
                "TextRange::new({:?}.into(), {:?}.into()), ",
                child.text_range().start(),
                child.text_range().end()
            );
        }
        println!("];");
    }

    fn check_ast_children(
        ast: &AstCircomProgram,
        expected_kinds: &Vec<TokenKind>,
        expected_ranges: &Vec<TextRange>,
    ) {
        let children = ast
            .syntax()
            .first_child()
            .unwrap()
            .siblings(rowan::Direction::Next);

        let mut kind_iterator = expected_kinds.iter();
        let mut range_iterator = expected_ranges.iter();

        for child in children {
            if let (Some(expected_kind), Some(expected_range)) =
                (kind_iterator.next(), range_iterator.next())
            {
                assert_eq!(child.kind(), *expected_kind);
                assert_eq!(child.text_range(), *expected_range);
            } else {
                panic!("Mismatched number of children and expected values");
            }
        }
        println!();
    }

    #[test]
    fn syntax_test_1() {
        let source: &str = test_programs::PARSER_TEST_1;

        let expected_pragma = "pragma circom 2.0.0;".to_string();
        let expected_kinds = vec![
            Pragma,
            EndLine,
            EndLine,
            WhiteSpace,
            EndLine,
            WhiteSpace,
            TemplateDef,
            EndLine,
            WhiteSpace,
            TemplateDef,
            WhiteSpace,
            EndLine,
            WhiteSpace,
        ];
        let expected_ranges = vec![
            TextRange::new(0.into(), 20.into()),
            TextRange::new(20.into(), 21.into()),
            TextRange::new(21.into(), 22.into()),
            TextRange::new(22.into(), 26.into()),
            TextRange::new(26.into(), 27.into()),
            TextRange::new(27.into(), 31.into()),
            TextRange::new(31.into(), 57.into()),
            TextRange::new(57.into(), 58.into()),
            TextRange::new(58.into(), 62.into()),
            TextRange::new(62.into(), 88.into()),
            TextRange::new(88.into(), 89.into()),
            TextRange::new(89.into(), 90.into()),
            TextRange::new(90.into(), 94.into()),
        ];

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        
        if let Some(ast) = AstCircomProgram::cast(syntax) {            
            check_ast_children(&ast, &expected_kinds, &expected_ranges);

            // check pragma
            let pragma = ast.pragma().unwrap().syntax().text().to_string();
            assert_eq!(pragma, expected_pragma, "Pragma is not correct!");

            // check ast hash
            let mut hasher = DefaultHasher::default();
            ast.syntax().hash(&mut hasher);
            let _ast_hash = hasher.finish();

            // check template hash
            let mut h1 = DefaultHasher::default();
            let mut h2 = DefaultHasher::default();

            let template = ast.template_list();

            template[0].syntax().hash(&mut h1);
            template[1].syntax().hash(&mut h2);

            assert_ne!(
                h1.finish(),
                h2.finish(),
                "Templates with same syntax should have different hashes!"
            );

            // check template syntax (text & green node)
            assert_eq!(
                template[0].syntax().text(),
                template[1].syntax().text(),
                "The syntax (as text) of template 1 and 2 must be the same!"
            );
            assert_eq!(
                template[0].syntax().green(),
                template[1].syntax().green(),
                "The syntax (as green node) of template 1 and 2 must be the same!!"
            );
        }
    }

    #[test]
    fn syntax_test_2() {
        let source = test_programs::PARSER_TEST_2;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            // print_ast_children(&ast);

            println!("Pragma: {:?}", ast.pragma().unwrap().syntax().text());

            print!("Templates: ");
            let templates = ast.template_list();
            for template in templates.iter() {
                // print!("{:?} ", template.name().unwrap().name().unwrap().syntax().text());
                print!("{:?} ", template.name().unwrap().syntax().text()); // leading whitespaces
                                                                           // print!("{:?} ", template.syntax().text()); // leading whitespaces
            }
            println!();

            print!("Functions: ");
            let functions = ast.function_list();
            for function in functions.iter() {
                print!("{:?} ", function.function_name().unwrap().syntax().text());
                // leading whitespaces
                // print!("{:?} ", function.syntax().text()); // leading whitespaces
            }
            println!();
        }
    }

    #[test]
    fn syntax_test_3() {
        let source = test_programs::PARSER_TEST_3;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            // print_ast_children(&ast);

            println!("Pragma: {:?}", ast.pragma().unwrap().syntax().text());
            println!(
                "Pragma version: {:?}",
                ast.pragma().unwrap().version().unwrap().syntax().text()
            );
        }
    }

    #[test]
    fn syntax_test_4() {
        let source = test_programs::PARSER_TEST_4;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            // print_ast_children(&ast);

            println!("Pragma: {:?}", ast.pragma().unwrap().syntax().text());
            println!(
                "Pragma version: {:?}",
                ast.pragma().unwrap().version().unwrap().syntax().text()
            );
        }
    }

    #[test]
    fn syntax_test_5() {
        let source = test_programs::PARSER_TEST_5;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            // print_ast_children(&ast);

            println!("{:?}", ast.pragma());
            // assert!(ast.pragma().is_none(), "No pragma in source code");
        }
    }

    #[test]
    fn syntax_test_6() {
        let source = test_programs::PARSER_TEST_6;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            // print_ast_children(&ast);

            println!("{:?}", ast.pragma());
            // assert!(ast.pragma().is_none(), "No pragma in source code");
        }
    }
}
