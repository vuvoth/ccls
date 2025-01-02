use parser::input::Input;
use parser::output::{Child, Output};
use parser::parser::Parser;
use parser::token_kind::TokenKind;
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
                Child::Error(error) => {
                    let token_kind = TokenKind::Error;
                    let token_value = error.as_str();

                    self.builder.start_node(token_kind.into());
                    self.builder.token(token_kind.into(), token_value);
                    self.builder.finish_node();
                }
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

    fn ast_from_source(source: &str) -> AstCircomProgram {
        let syntax = SyntaxTreeBuilder::syntax_tree(source);
        AstCircomProgram::cast(syntax).unwrap()
    }

    fn children_from_ast(ast: &AstCircomProgram) -> Vec<String> {
        let children = ast
            .syntax()
            .first_child()
            .unwrap()
            .siblings(rowan::Direction::Next)
            .into_iter()
            .map(|child| child.text().to_string())
            .collect();

        children
    }

    fn pragma_string_from_ast(ast: &AstCircomProgram) -> String {
        ast.pragma().unwrap().syntax().text().to_string()
    }

    fn pragma_version_from_ast(ast: &AstCircomProgram) -> String {
        ast.pragma()
            .unwrap()
            .version()
            .unwrap()
            .syntax()
            .text()
            .to_string()
    }

    fn template_names_from_ast(ast: &AstCircomProgram) -> Vec<String> {
        let templates = ast
            .template_list()
            .iter()
            .map(|template| template.name().unwrap().syntax().text().to_string())
            .collect();

        templates
    }

    fn function_names_from_ast(ast: &AstCircomProgram) -> Vec<String> {
        let functions = ast
            .function_list()
            .iter()
            .map(|function| {
                function
                    .function_name()
                    .unwrap()
                    .syntax()
                    .text()
                    .to_string()
            })
            .collect();

        functions
    }

    #[test]
    fn syntax_test_1() {
        let ast = ast_from_source(test_programs::PARSER_TEST_1);

        // check_ast_children
        let children = children_from_ast(&ast);
        insta::assert_yaml_snapshot!("syntax_test_1_children", children);

        // check pragma
        let pragma = pragma_string_from_ast(&ast);
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

    #[test]
    fn syntax_test_2() {
        let ast = ast_from_source(test_programs::PARSER_TEST_2);

        let pragma = pragma_string_from_ast(&ast);
        insta::assert_yaml_snapshot!(pragma, @"pragma circom 2.0.0;");

        let function_names = function_names_from_ast(&ast);
        insta::assert_yaml_snapshot!("syntax_test_2_functions", function_names);

        let template_names = template_names_from_ast(&ast);
        insta::assert_yaml_snapshot!("syntax_test_2_templates", template_names);
    }

    #[test]
    fn syntax_test_3() {
        let ast = ast_from_source(test_programs::PARSER_TEST_3);
        let pragma = pragma_string_from_ast(&ast);
        insta::assert_yaml_snapshot!(pragma, @"pragma circom 2.0.0;");

        let pragma_version = pragma_version_from_ast(&ast);
        insta::assert_yaml_snapshot!(pragma_version, @"2.0.0");
    }

    #[test]
    fn syntax_test_4() {
        let ast = ast_from_source(test_programs::PARSER_TEST_4);
        let pragma = pragma_string_from_ast(&ast);
        insta::assert_yaml_snapshot!(pragma, @"pragma circom 2.0.0;");

        let pragma_version = pragma_version_from_ast(&ast);
        insta::assert_yaml_snapshot!(pragma_version, @"2.0.0");
    }

    #[test]
    fn syntax_test_5() {
        let ast = ast_from_source(test_programs::PARSER_TEST_5);
        let pragma = ast.pragma().is_none();
        insta::assert_yaml_snapshot!(pragma, @"true");

        let template_names = template_names_from_ast(&ast);
        insta::assert_yaml_snapshot!("syntax_test_5_templates", template_names);
    }

    #[test]
    fn syntax_test_6() {
        let ast = ast_from_source(test_programs::PARSER_TEST_6);
        let pragma = ast.pragma().is_none();
        insta::assert_yaml_snapshot!(pragma, @"true");

        let template_names = template_names_from_ast(&ast);
        insta::assert_yaml_snapshot!("syntax_test_6_templates", template_names);
    }
}

#[cfg(test)]
mod grammar_tests {

    use crate::{
        abstract_syntax_tree::{AstBlock, AstOutputSignalDecl, AstPragma, AstTemplateDef},
        syntax::SyntaxTreeBuilder,
        syntax_node::CircomLanguage,
    };
    use parser::{grammar::entry::Scope, input::Input, parser::Parser};
    use rowan::{ast::AstNode, SyntaxNode};

    fn syntax_node_from_source(source: &str, scope: Scope) -> SyntaxNode<CircomLanguage> {
        let input = Input::new(&source);
        let output = Parser::parsing_with_scope(&input, scope);

        // output is a tree whose node is index of token, no content of token
        // convert output into green node
        let mut builder = SyntaxTreeBuilder::new(&input);
        builder.build(output);
        let green = builder.finish();

        // then cast green node into syntax node
        let syntax = SyntaxNode::new_root(green);

        syntax
    }

    #[test]
    fn pragma_happy_test() {
        // parse source (string) into output tree
        let version = r#"2.0.1"#;
        let source = format!(r#"pragma circom {};"#, version);

        let syntax = syntax_node_from_source(&source, Scope::Pragma);

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

        let syntax = syntax_node_from_source(&SOURCE, Scope::Template);

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
        let statements = template.statements().unwrap();
        let output_signal = statements.find_children::<AstOutputSignalDecl>();
        println!("{:?}", output_signal);

        let statements: Vec<String> = statements
            .statement_list()
            .into_iter()
            .map(|statement| statement.syntax().text().to_string())
            .collect();
        insta::assert_yaml_snapshot!("template_happy_test_statements", statements);

        // input signal
        let input_signal = template
            .find_input_signal("in")
            .unwrap()
            .syntax()
            .text()
            .to_string();
        insta::assert_yaml_snapshot!(input_signal, @r###""signal input in[N]""###);

        // output signal
        let output_signal = template
            .find_output_signal("out")
            .unwrap()
            .syntax()
            .text()
            .to_string();
        insta::assert_yaml_snapshot!(output_signal, @"signal output out");

        // internal signal
        let internal_signal = template.find_internal_signal("in").is_none();
        insta::assert_yaml_snapshot!(internal_signal, @"true");

        // component
        let component = template
            .find_component("comp")
            .unwrap()
            .syntax()
            .text()
            .to_string();
        insta::assert_yaml_snapshot!(component, @r###""component comp[N-1]""###);
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

        let syntax = syntax_node_from_source(&source, Scope::Block);

        // cast syntax node into ast node to retrieve more information
        let block = AstBlock::cast(syntax).expect("Can not cast syntax node into ast block");

        println!("block: {}", block.syntax().text().to_string());

        // finally, assert with expect statements
        let statements = block.statement_list().unwrap().statement_list();
        let statements: Vec<String> = statements
            .into_iter()
            .map(|statement| statement.syntax().text().to_string())
            .collect();
        insta::assert_yaml_snapshot!("block_happy_test_statements", statements);
    }

    #[test]
    fn statement_happy_test() {
        let source = r#"{
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

            // just for testing statement
            while (out) {
                for(var i = 0; i < N-1; i++){
                    comp[i] = Multiplier2();
                }
            }
            assert(comp);
            log("Print something...", out);
     
            if (1 < 2) {
                log("Match...", 1 < 2);
            } else {
                log("Does not match...", 1 < 2);
            }
            
            return out + comp;
        }"#;

        let syntax = syntax_node_from_source(&source, Scope::Block);

        // cast syntax node into ast node to retrieve more information
        let block = AstBlock::cast(syntax).expect("Can not cast syntax node into ast block");

        let statements = block.statement_list().unwrap().statement_list();
        let statements: Vec<String> = statements
            .into_iter()
            .map(|statement| statement.syntax().text().to_string())
            .collect();
        insta::assert_yaml_snapshot!("statement_happy_test_statements", statements);
    }

    #[test]
    fn declaration_happy_test() {
        // [scope: block] source must start with {
        let source = r#"{
            var nout = nbits((2**n -1)*ops);
            signal input in[ops][n];
            signal output out[nout];
        
            var lin = 0;
            var lout = 0;
        
            var k;
            var j;
        
            var e2;
        
            e2 = 1;
            for (k=0; k<n; k++) {
                for (j=0; j<ops; j++) {
                    lin += in[j][k] * e2;
                }
                e2 = e2 + e2;
            
                e2 = 1;
                for (k=0; k<nout; k++) {
                    out[k] <-- (lin >> k) & 1;
            
                    // Ensure out is binary
                    // out[k] * (out[k] - 1) === 0;
            
                    lout += out[k] * e2;
            
                    e2 = e2+e2;
                }
            
                // Ensure the sum;
            
                lin === lout;
            }
    }"#;

        let syntax = syntax_node_from_source(&source, Scope::Block);

        // cast syntax node into ast node to retrieve more information
        let block = AstBlock::cast(syntax).expect("Can not cast syntax node into ast block");

        let string_syntax = block.syntax().text().to_string();
        insta::assert_yaml_snapshot!("declaration_happy_test_source", string_syntax);
    }
}
