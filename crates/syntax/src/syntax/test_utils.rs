pub use rowan::{NodeOrToken, WalkEvent};

use crate::syntax_node::SyntaxNode;

#[macro_export]
macro_rules! test_syntax {
    ($file_path:expr) => {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let workspace_path = std::path::Path::new(&crate_path)
            .parent()
            .and_then(|p| p.parent())
            .expect("Failed to find workspace root");

        let full_path = workspace_path.join($file_path.trim_start_matches('/'));
        let source =
            std::fs::read_to_string(&full_path).expect(&format!("Failed to read {:?}", full_path));
        let syntax = crate::syntax::syntax_node_from_source(&source);
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
                            "{} {} {:?}",
                            level_str(level),
                            node.kind(),
                            node.text_range()
                        ));
                    }
                    NodeOrToken::Token(token) => {
                        result.push_str(&format!(
                            "{} {} {:?} {:?}",
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
    let mut ans = String::new();

    for _i in 0..level {
        ans.push_str("|     ");
    }
    ans
}
