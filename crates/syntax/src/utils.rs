#[macro_export]
macro_rules! test_syntax {
    ($file_path:expr, $scope: expr) => {
        let crate_path = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let full_path = format!("{}{}", crate_path, $file_path);
        let source = std::fs::read_to_string(full_path).expect("Should not failed");
        let syntax = crate::syntax::syntax_node_from_source(&source, $scope);
        insta::assert_snapshot!($file_path, crate::view_syntax::view_ast(&syntax));
    };
}
