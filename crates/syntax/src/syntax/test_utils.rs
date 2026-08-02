pub use rowan::{NodeOrToken, WalkEvent};

use crate::syntax_node::SyntaxNode;

#[macro_export]
macro_rules! test_syntax {
    ($file_path:expr, $scope: expr) => {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();

        let full_path = format!("{}{}", crate_path, $file_path);
        let source = std::fs::read_to_string(full_path).expect("Should not failed");
        let syntax = $crate::syntax::syntax_node_from_source(&source, $scope);
        insta::assert_snapshot!($file_path, view_ast(&syntax));
    };
}

pub fn view_ast(node: &SyntaxNode) -> String {
    let mut level = 0;
    let mut result = String::new();
    for event in node.preorder_with_tokens() {
        match event {
            WalkEvent::Enter(it) => {
                match it {
                    NodeOrToken::Node(node) => {
                        result.push_str(&format!(
                            "{} {:?} {:?}",
                            level_str(level),
                            node.kind(),
                            node.text_range()
                        ));
                    }
                    NodeOrToken::Token(token) => {
                        result.push_str(&format!(
                            "{} {:?} {:?} {:?}",
                            level_str(level),
                            token.kind(),
                            token.text_range(),
                            token.text()
                        ));
                    }
                }
                result.push('\n');
                level += 1;
            }

            WalkEvent::Leave(_it) => {
                level -= 1;
            }
        }
    }
    result
}

fn level_str(level: u32) -> String {
    let mut ans = String::from("");

    for _i in 0..level {
        ans.push_str("|     ");
    }
    ans
}
