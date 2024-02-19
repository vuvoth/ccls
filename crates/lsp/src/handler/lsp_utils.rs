use std::path::{self, Path, PathBuf};

use lsp_types::{Position, Range, Url};
use parser::syntax_node::SyntaxNode;
use rowan::TextSize;

pub struct FileId(u32);

pub struct FileUtils {
    pub file_id: FileId,
    pub file_path: Url,
    pub end_line_vec: Vec<u32>,
}

impl FileUtils {
    pub fn create(content: &str, file_path: Url) -> Self {
        Self::new(FileId(0), content, file_path)
    }

    pub(super) fn new(file_id: FileId, content: &str, file_path: Url) -> Self {
        let mut file_utils = Self {
            file_id,
            file_path,
            end_line_vec: Vec::new(),
        };

        for (id, c) in content.chars().enumerate() {
            if c == '\n' {
                file_utils.end_line_vec.push(id as u32);
            }
        }

        file_utils
    }

    pub fn get_path(&self) -> PathBuf {
        let p = self.file_path.path();
        PathBuf::from(p)
    }

    pub fn off_set(&self, position: Position) -> TextSize {
        if position.line == 0 {
            return position.character.into();
        }
        (self.end_line_vec[position.line as usize - 1] + position.character + 1).into()
    }

    pub fn position(&self, off_set: TextSize) -> Position {
        let line = match self.end_line_vec.binary_search(&(off_set.into())) {
            Ok(l) => l,
            Err(l) => l,
        };

        Position::new(
            line as u32,
            if line > 0 {
                (u32::from(off_set)) - self.end_line_vec[line - 1] - 1
            } else {
                off_set.into()
            },
        )
    }

    pub fn range(&self, syntax: &SyntaxNode) -> Range {
        let syntax_range = syntax.text_range();
        Range {
            start: self.position(syntax_range.start()),
            end: self.position(syntax_range.end()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{Position, Url};

    use crate::handler::lsp_utils::{FileId, FileUtils};

    #[test]
    fn off_set_test() {
        let str = r#"
one
two
three
       "#;

        let file_utils = FileUtils::new(
            FileId(1),
            str,
            Url::from_file_path(Path::new("tmp.txt")).unwrap(),
        );

        let position = Position::new(0, 1);

        assert_eq!(file_utils.off_set(position), 1.into());

        let position = Position::new(1, 1);

        assert_eq!(file_utils.off_set(position), 2.into());
    }

    #[test]
    fn position_test() {
        let str = r#"
        one
        two
        three
               "#;

        // 0, 4, 8
        let file_utils = FileUtils::new(
            FileId(1),
            str,
            Url::from_file_path(Path::new("tmp.txt")).unwrap(),
        );
        assert_eq!(Position::new(1, 1), file_utils.position(2.into()));
        assert_eq!(Position::new(0, 0), file_utils.position(0.into()));
    }
}
