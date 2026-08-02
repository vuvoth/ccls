use std::path::PathBuf;

use lsp_types::{Position, Range, Url};
use rowan::TextSize;

use syntax::syntax_node::SyntaxNode;

// File identity is owned by the `vfs` crate (a path-interned `u32` index), so aliased paths
// collapse to one id via interning rather than via a path hash. Re-exported here for callers that
// reach it via `crate::file_db::FileId`.
pub use vfs::FileId;

/// Per-file offset/line bookkeeping over a document's source text. Carries the [`FileId`] (owned by
/// the VFS), the canonical `file:` URL, and the byte offsets of every `\n` so positions and offsets
/// can be converted in either direction. The range helper turns a syntax node's byte range into an
/// LSP [`Range`].
#[derive(Clone)]
pub struct FileDB {
    pub file_id: FileId,
    pub file_path: Url,
    pub newline_offsets: Vec<u32>,
}

impl FileDB {
    pub(crate) fn new(file_id: FileId, content: &str, file_path: Url) -> Self {
        let mut file_db = Self {
            file_id,
            file_path,
            newline_offsets: Vec::new(),
        };

        for (offset, c) in content.chars().enumerate() {
            if c == '\n' {
                file_db.newline_offsets.push(offset as u32);
            }
        }

        file_db
    }

    pub fn get_path(&self) -> PathBuf {
        let p = self.file_path.path();
        PathBuf::from(p)
    }

    pub fn offset(&self, position: Position) -> TextSize {
        if position.line == 0 || self.newline_offsets.is_empty() {
            return position.character.into();
        }
        // Clamp a line past EOF to the last known line instead of indexing out of range.
        let idx = (position.line as usize).min(self.newline_offsets.len()) - 1;
        (self.newline_offsets[idx] + position.character + 1).into()
    }

    pub fn position(&self, offset: TextSize) -> Position {
        let line = match self.newline_offsets.binary_search(&(offset.into())) {
            Ok(l) => l,
            Err(l) => l,
        };

        Position::new(
            line as u32,
            if line > 0 {
                (u32::from(offset)) - self.newline_offsets[line - 1] - 1
            } else {
                offset.into()
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

    use super::{FileDB, FileId};

    #[test]
    fn offset_test() {
        let source = r#"
one
two
three
       "#;

        let file_db = FileDB::new(
            FileId(1),
            source,
            Url::from_file_path(Path::new("/tmp.txt")).unwrap(),
        );

        let position = Position::new(0, 1);

        assert_eq!(file_db.offset(position), 1.into());

        let position = Position::new(1, 1);

        assert_eq!(file_db.offset(position), 2.into());
    }

    #[test]
    fn position_test() {
        let source = r#"
        one
        two
        three
               "#;

        // newline byte offsets: 0, 12, 24 (the leading `\n` then the indented lines)
        let file_db = FileDB::new(
            FileId(1),
            source,
            Url::from_file_path(Path::new("/tmp.txt")).unwrap(),
        );
        assert_eq!(Position::new(1, 1), file_db.position(2.into()));
        assert_eq!(Position::new(0, 0), file_db.position(0.into()));
    }
}
