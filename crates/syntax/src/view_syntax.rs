use crate::syntax_node::SyntaxNode;

pub use rowan::{NodeOrToken, WalkEvent};

fn level_str(level: u32) -> String {
    let mut ans = String::from("");

    for _i in 0..level {
        ans.push_str("|     ");
    }
    ans
}

pub(crate) fn view_ast(node: &SyntaxNode) -> String {
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
                level = level + 1;
            }

            WalkEvent::Leave(_it) => {
                level = level - 1;
            }
        }
    }
    return result;
}
