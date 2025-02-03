use parser::grammar::entry::Scope;
use parser::input::Input;
use parser::output::{Child, Output};
use parser::parser::Parser;
use parser::token_kind::TokenKind;
use rowan::{GreenNode, GreenNodeBuilder};

pub use rowan::{
    api::Preorder, Direction, NodeOrToken, SyntaxText, TextRange, TextSize, TokenAtOffset,
    WalkEvent,
};

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

pub fn syntax_node_from_source(source: &str, scope: Scope) -> SyntaxNode {
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

#[cfg(test)]
mod tests {
    use crate::test_syntax;
    use parser::grammar::entry::Scope;

    #[test]
    fn pragma_happy_test() {
        test_syntax!("/src/test_files/happy/pragma.circom", Scope::Pragma);
    }

    #[test]
    fn template_happy_test() {
        // SOURCE & EXPECTED RESULT
        test_syntax!("/src/test_files/happy/template.circom", Scope::Template);
    }

    #[test]
    fn block_happy_test() {
        test_syntax!("/src/test_files/happy/block.circom", Scope::Block);
    }

    #[test]
    fn comment_happy_test() {
        test_syntax!(
            "/src/test_files/happy/block_comment.circom",
            Scope::CircomProgram
        );
        test_syntax!(
            "/src/test_files/happy/line_comment.circom",
            Scope::CircomProgram
        );
    }

    #[test]
    fn full_circom_program() {
        test_syntax!(
            "/src/test_files/happy/full_circom_program.circom",
            Scope::CircomProgram
        );
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
            for (k=0; k<n; k+=1) {
                for (j=0; j<ops; j++) {
                    lin += in[j][k] * e2;
                }
                e2 = e2 + e2;
            
                e2 = 1;
                for (k=0; k<nout; ++k) {
                    out[k] <-- (lin >> k) & 1;
            
                    // Ensure out is binary
                    out[k] * (out[k] - 1) === 0;
            
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

    #[test]
    fn function_happy_test() {
        let source = r#"
        function nbits(a) {
            var n = 1;
            var r = 0;
            while (n-1<a) {
                r++;
                n *= 2;
            }
            return r;
        }"#;

        let syntax = syntax_node_from_source(&source, Scope::CircomProgram);

        // cast syntax node into ast node to retrieve more information
        let ast_circom =
            AstCircomProgram::cast(syntax).expect("Can not cast syntax node into ast circom");
        let function = &ast_circom.function_list()[0];

        let string_function = function.syntax().text().to_string();
        insta::assert_yaml_snapshot!("function_happy_test_source", string_function);
    }
}
