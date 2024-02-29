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
        let mut builder = SyntaxTreeBuilder::new(&input);
        let output = Parser::parsing(&input);
        builder.build(output);
        let green = builder.finish();
        SyntaxNode::new_root(green)
    }
}

#[cfg(test)]
mod tests {

    use std::hash::{DefaultHasher, Hash, Hasher};

    use rowan::ast::AstNode;

    use crate::abstract_syntax_tree::AstCircomProgram;

    use super::SyntaxTreeBuilder;

    #[test]
    fn other_parser_test() {
        let source: String = r#"pragma circom 2.0.0;

        
        template Multiplier2 () {}
        template Multiplier2 () {} 
        "#
        .to_string();

        let syntax = SyntaxTreeBuilder::syntax_tree(&source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            let mut hasher = DefaultHasher::default();
            ast.syntax().hash(&mut hasher);
            // println!("{:#?}", syntax);
            println!("{:?}", hasher.finish());

            let mut h1 = DefaultHasher::default();

            let mut h2 = DefaultHasher::default();

            let template = ast.template_list();

            template[0].syntax().hash(&mut h1);
            template[1].syntax().hash(&mut h2);

            println!("{}", h1.finish());
            println!("{}", h2.finish());
            println!("{:?}", template[0].syntax().text());
            println!("{:?}", template[1].syntax().text());
            println!("{}", template[0].syntax() == template[0].syntax());
            println!(
                "{}",
                template[0].syntax().green() == template[1].syntax().green()
            );
        }

        // find token
    }


    #[test]
    fn parser_test() {
        let source = r#"/*
        Copyright 2018 0KIMS association.
    
        This file is part of circom (Zero Knowledge Circuit Compiler).
    
        circom is a free software: you can redistribute it and/or modify it
        under the terms of the GNU General Public License as published by
        the Free Software Foundation, either version 3 of the License, or
        (at your option) any later version.
    
        circom is distributed in the hope that it will be useful, but WITHOUT
        ANY WARRANTY; without even the implied warranty of MERCHANTABILITY
        or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public
        License for more details.
    
        You should have received a copy of the GNU General Public License
        along with circom. If not, see <https://www.gnu.org/licenses/>.
    */
    /*
    
    Binary Sum
    ==========
    
    This component creates a binary sum componet of ops operands and n bits each operand.
    
    e is Number of carries: Depends on the number of operands in the input.
    
    Main Constraint:
       in[0][0]     * 2^0  +  in[0][1]     * 2^1  + ..... + in[0][n-1]    * 2^(n-1)  +
     + in[1][0]     * 2^0  +  in[1][1]     * 2^1  + ..... + in[1][n-1]    * 2^(n-1)  +
     + ..
     + in[ops-1][0] * 2^0  +  in[ops-1][1] * 2^1  + ..... + in[ops-1][n-1] * 2^(n-1)  +
     ===
       out[0] * 2^0  + out[1] * 2^1 +   + out[n+e-1] *2(n+e-1)
    
    To waranty binary outputs:
    
        out[0]     * (out[0] - 1) === 0
        out[1]     * (out[0] - 1) === 0
        .
        .
        .
        out[n+e-1] * (out[n+e-1] - 1) == 0
    
     */
    
    
    /*
        This function calculates the number of extra bits in the output to do the full sum.
     */
     pragma circom 2.0.0;
    
    function nbits(a) {
        var n = 1;
        var r = 0;
        while (n-1<a) {
            r++;
            n *= 2;
        }
        return r;
    }
    
    
    template BinSum(n, ops) {
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
        }
    
        e2 = 1;
        for (k=0; k<nout; k++) {
            out[k] <-- (lin >> k) & 1;
    
            // Ensure out is binary
            out[k] * (out[k] - 1) === 0;
    
            lout += out[k] * e2;
    
            e2 = e2+e2;
        }
    
        // Ensure the sum;
    
        lin === lout;
    }
    "#;

        let syntax = SyntaxTreeBuilder::syntax_tree(&source);
    }
}
