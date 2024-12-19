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
                    // TODO: return Error to replace .unwrap()
                    let token_value = self.input.token_value(*token_id).unwrap();
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
    use std::hash::{DefaultHasher, Hash, Hasher};

    use rowan::ast::AstNode;

    use crate::{abstract_syntax_tree::AstCircomProgram, test_programs};

    use super::SyntaxTreeBuilder;

    #[test]
    fn syntax_test_1() {
        let source: &str = test_programs::PARSER_TEST_1;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            // check_ast_children
            let children = ast
                .syntax()
                .first_child()
                .unwrap()
                .siblings(rowan::Direction::Next);
            let mut children_string = Vec::new();

            for child in children.into_iter() {
                children_string.push(child.text().to_string());
            }
            insta::assert_yaml_snapshot!("syntax_test_1_children", children_string);

            // check pragma
            let pragma = ast.pragma().unwrap().syntax().text().to_string();
            insta::assert_yaml_snapshot!(pragma, @"pragma circom 2.0.0;");

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
            let pragma = ast.pragma().unwrap().syntax().text().to_string();
            insta::assert_yaml_snapshot!(pragma, @"pragma circom 2.0.0;");

            let templates = ast.template_list();
            let mut template_names = Vec::new();
            for template in templates.iter() {
                let template_name = template.name().unwrap().syntax().text().to_string();
                template_names.push(template_name);
            }
            insta::assert_yaml_snapshot!("syntax_test_2_templates", template_names);

            let functions = ast.function_list();
            let mut function_names = Vec::new();
            for function in functions.iter() {
                let function_name = function
                    .function_name()
                    .unwrap()
                    .syntax()
                    .text()
                    .to_string();
                function_names.push(function_name);
            }
            insta::assert_yaml_snapshot!("syntax_test_2_functions", function_names);
        }
    }

    #[test]
    fn syntax_test_3() {
        let source = test_programs::PARSER_TEST_3;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            let pragma = ast.pragma().unwrap().syntax().text().to_string();
            insta::assert_yaml_snapshot!(pragma, @"pragma circom 2.0.0;");

            let pragma_version = ast
                .pragma()
                .unwrap()
                .version()
                .unwrap()
                .syntax()
                .text()
                .to_string();
            insta::assert_yaml_snapshot!(pragma_version, @"2.0.0");
        }
    }

    #[test]
    fn syntax_test_4() {
        let source = test_programs::PARSER_TEST_4;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            let pragma = ast.pragma().unwrap().syntax().text().to_string();
            insta::assert_yaml_snapshot!(pragma, @"pragma circom 2.0.0;");

            let pragma_version = ast
                .pragma()
                .unwrap()
                .version()
                .unwrap()
                .syntax()
                .text()
                .to_string();
            insta::assert_yaml_snapshot!(pragma_version, @"2.0.0");
        }
    }

    #[test]
    fn syntax_test_5() {
        let source = test_programs::PARSER_TEST_5;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            let pragma = ast.pragma().is_none();
            insta::assert_yaml_snapshot!(pragma, @"true");

            let templates = ast.template_list();
            let mut template_names = Vec::new();
            for template in templates.iter() {
                let template_name = template.name().unwrap().syntax().text().to_string();
                template_names.push(template_name);
            }
            insta::assert_yaml_snapshot!("syntax_test_5_templates", template_names);
        }
    }

    #[test]
    fn syntax_test_6() {
        let source = test_programs::PARSER_TEST_6;

        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            let pragma = ast.pragma().is_none();
            insta::assert_yaml_snapshot!(pragma, @"true");

            let templates = ast.template_list();
            let mut template_names = Vec::new();
            for template in templates.iter() {
                let template_name = template.name().unwrap().syntax().text().to_string();
                template_names.push(template_name);
            }
            insta::assert_yaml_snapshot!("syntax_test_6_templates", template_names);
        }
    }
}

#[cfg(test)]
mod grammar_tests {
    use parser::{grammar::entry::Scope, input::Input, parser::Parser};
    use rowan::ast::AstNode;

    use crate::{
        abstract_syntax_tree::{AstBlock, AstPragma, AstTemplateDef},
        syntax::SyntaxTreeBuilder,
        syntax_node::SyntaxNode,
    };

    #[test]
    fn pragma_happy_test() {
        // parse source (string) into output tree
        let version = r#"2.0.1"#;
        let source = format!(r#"pragma circom {};"#, version);
        let input = Input::new(&source);
        let output = Parser::parsing_with_scope(&input, Scope::Pragma);

        // output is a tree whose node is index of token, no content of token
        // convert output into green node
        let mut builder = SyntaxTreeBuilder::new(&input);
        builder.build(output);
        let green = builder.finish();

        // then cast green node into syntax node
        let syntax = SyntaxNode::new_root(green);

        // cast syntax node into ast node to retrieve more information
        let pragma = AstPragma::cast(syntax).expect("Can not cast syntax node into ast pragma");

        // finally, assert with expect value
        let pragma_versison_kind = pragma.version().unwrap().syntax().kind();
        insta::assert_yaml_snapshot!(pragma_versison_kind, @"Version");

        let pragma_versison_text = pragma.version().unwrap().syntax().text().to_string();
        insta::assert_yaml_snapshot!(pragma_versison_text, @"2.0.1");
    }

    #[test]
    fn template_happy_test() {
        // SOURCE & EXPECTED RESULT
        const SOURCE: &str = r#"template MultiplierN (N, P, QQ) {
            //Declaration of signals and components.
            signal input in[N];
            signal output out;
            component comp[N-1];
            
            //Statements.
            for(var i = 0; i < N-1; i++){
                comp[i] = Multiplier2();
                }
                
                // ... some more code (see below)
                
                }"#;

        // parse source (string) into output tree
        let input = Input::new(&SOURCE);
        let output = Parser::parsing_with_scope(&input, Scope::Template);

        // output is a tree whose node is index of token, no content of token
        // convert output into green node
        let mut builder = SyntaxTreeBuilder::new(&input);
        builder.build(output);
        let green = builder.finish();

        // then cast green node into syntax node
        let syntax = SyntaxNode::new_root(green);

        // cast syntax node into ast node to retrieve more information
        let template =
            AstTemplateDef::cast(syntax).expect("Can not cast syntax node into ast template");

        // finally, assert with expect value

        // name
        let name = template
            .name()
            .expect("Can not extract template name")
            .syntax()
            .text()
            .to_string();
        insta::assert_yaml_snapshot!(name, @"MultiplierN");

        // parameter list
        let first_param = template
            .parameter_list()
            .expect("Can not detect parameter list")
            .syntax()
            .first_child()
            .unwrap()
            .text()
            .to_string();
        insta::assert_yaml_snapshot!(first_param, @"N");

        let last_param = template
            .parameter_list()
            .expect("Can not detect parameter list")
            .syntax()
            .last_child()
            .unwrap()
            .text()
            .to_string();
        insta::assert_yaml_snapshot!(last_param, @"QQ");

        // statements
        let statements = template.statements().unwrap().statement_list();
        let statements: Vec<String> = statements
            .into_iter()
            .map(|statement| statement.syntax().text().to_string())
            .collect();
        insta::assert_yaml_snapshot!("template_happy_test_statements", statements);

        // // input signal
        // println!("find_input_signal: {:?}", template.find_input_signal());

        // // output signal
        // println!("find_output_signal: {:?}", template.find_output_signal());

        // // internal signal
        // println!("find_internal_signal: {:?}", template.find_internal_signal());

        // // component
        // println!("find_component: {:?}", template.find_component());
    }

    #[test]
    fn block_happy_test() {
        // SOURCE & EXPECTED RESULT
        let source = r#"{
            //Declaration of signals.
            signal input in[N];
            signal output out;
            component comp[N-1];

            //Statements.
            for(var i = 0; i < N-1; i++){
                comp[i] = Multiplier2();
            }
            comp[0].in1 <== in[0];
            comp[0].in2 <== in[1];
            for(var i = 0; i < N-2; i++){
                comp[i+1].in1 <== comp[i].out;
                comp[i+1].in2 <== in[i+2];

            }
            out <== comp[N-2].out; 
        }"#;
        /*
        let expected_statements = vec![
            "signal input in[N];",
            "signal output out;",
            "component comp[N-1];",
            "for(var i = 0; i < N-1; i++){
                comp[i] = Multiplier2();
            }",
            "comp[0].in1 <== in[0];",
            "comp[0].in2 <== in[1];",
            "for(var i = 0; i < N-2; i++){
                comp[i+1].in1 <== comp[i].out;
                comp[i+1].in2 <== in[i+2];

            }",
            "out <== comp[N-2].out;",
        ];
        */

        // parse source (string) into output tree
        let input = Input::new(&source);
        let output = Parser::parsing_with_scope(&input, Scope::Block);

        // output is a tree whose node is index of token, no content of token
        // convert output into green node
        let mut builder = SyntaxTreeBuilder::new(&input);
        builder.build(output);
        let green = builder.finish();

        // then cast green node into syntax node
        let syntax = SyntaxNode::new_root(green);

        // cast syntax node into ast node to retrieve more information
        let block = AstBlock::cast(syntax).expect("Can not cast syntax node into ast block");

        // finally, assert with expect statements
        let statements = block.statement_list().unwrap().statement_list();
        let statements: Vec<String> = statements
            .into_iter()
            .map(|statement| statement.syntax().text().to_string())
            .collect();
        insta::assert_yaml_snapshot!("block_happy_test_statements", statements);
    }
}
