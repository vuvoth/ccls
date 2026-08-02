use std::path::PathBuf;
use std::sync::Arc;

use lsp_types::{Position, Range, Url};
use rowan::TextSize;

use syntax::syntax_node::{SyntaxNode, SyntaxToken};

// File identity is owned by the `vfs` crate (a path-interned `u32` index), so aliased paths
// collapse to one id via interning rather than via a path hash. Re-exported here for callers that
// reach it via `crate::file_db::FileId`.
pub use vfs::FileId;

/// Per-file offset/line bookkeeping over a document's source text. Carries the [`FileId`] (owned by
/// the VFS), the canonical `file:` URL, the byte offsets of every `\n`, and the source text (for
/// UTF-16↔byte conversion — LSP positions are UTF-16 code-unit counts, not bytes). The range helper
/// turns a syntax node's byte range into an LSP [`Range`].
#[derive(Clone)]
pub struct FileDB {
    pub file_id: FileId,
    pub file_path: Url,
    pub newline_offsets: Vec<u32>,
    content: Arc<str>,
}

impl FileDB {
    pub(crate) fn new(file_id: FileId, content: &str, file_path: Url) -> Self {
        let mut newline_offsets = Vec::new();
        for (offset, c) in content.chars().enumerate() {
            if c == '\n' {
                newline_offsets.push(offset as u32);
            }
        }

        Self {
            file_id,
            file_path,
            newline_offsets,
            content: Arc::from(content),
        }
    }

    pub fn get_path(&self) -> PathBuf {
        let p = self.file_path.path();
        PathBuf::from(p)
    }

    /// Byte offset of an LSP [`Position`]. LSP `character` is a **UTF-16** code-unit count, so it is
    /// converted by walking the line's code points from the line start (not by adding it as a raw
    /// byte offset — that is only correct for pure ASCII). A `character` past the line end is
    /// clamped to the line's last position.
    pub fn offset(&self, position: Position) -> TextSize {
        let target = position.character;
        let line_start = self.line_start_byte(position.line);

        // Walk the line's code points, counting UTF-16 units, until we've consumed `target` of them
        // (or hit the line's newline / EOF). `result` is the byte offset after the last consumed
        // code point; for `target == 0` it stays at the line start.
        let mut utf16_seen: u32 = 0;
        let mut result = line_start;
        for (rel, ch) in self.content[line_start..].char_indices() {
            if ch == '\n' || utf16_seen >= target {
                break;
            }
            utf16_seen += ch.len_utf16() as u32;
            result = line_start + rel + ch.len_utf8();
        }
        (result as u32).into()
    }

    /// LSP [`Position`] of a byte `offset`. The line is found from the newline table (byte-based,
    /// exact); the `character` is the count of UTF-16 code units from the line start to `offset`.
    pub fn position(&self, offset: TextSize) -> Position {
        let offset = u32::from(offset) as usize;
        let line = match self.newline_offsets.binary_search(&(offset as u32)) {
            Ok(l) => l,
            Err(l) => l,
        };
        let line_start = self.line_start_byte(line as u32);
        // `line_start` and `offset` are both char boundaries (rowan token offsets always are, and
        // line starts follow a `\n`), so this slice is safe.
        let character = self.content[line_start..offset]
            .chars()
            .map(|c| c.len_utf16() as u32)
            .sum();
        Position::new(line as u32, character)
    }

    pub fn range(&self, syntax: &SyntaxNode) -> Range {
        let syntax_range = syntax.text_range();
        Range {
            start: self.position(syntax_range.start()),
            end: self.position(syntax_range.end()),
        }
    }

    /// LSP [`Range`] of a leaf [`SyntaxToken`] (rename/reference edits target the token span, not a
    /// wrapping node).
    pub fn token_range(&self, token: &SyntaxToken) -> Range {
        let r = token.text_range();
        Range {
            start: self.position(r.start()),
            end: self.position(r.end()),
        }
    }

    /// Byte offset of the first character of `line` (0 for line 0; one past the previous `\n`
    /// otherwise), clamped to a known line.
    fn line_start_byte(&self, line: u32) -> usize {
        if line == 0 || self.newline_offsets.is_empty() {
            return 0;
        }
        let idx = (line as usize).min(self.newline_offsets.len()) - 1;
        self.newline_offsets[idx] as usize + 1
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{Position, Url};

    use super::{FileDB, FileId};

    #[test]
    fn offset_test() {
        // Source begins with a '\n', so line 0 is empty and line 1 is "one".
        let source = "\none\ntwo\nthree\n";

        let file_db = FileDB::new(
            FileId(1),
            source,
            Url::from_file_path(Path::new("/tmp.txt")).unwrap(),
        );

        // Line 1 ("one") starts at byte 1; character 1 -> byte 2 ('n').
        assert_eq!(file_db.offset(Position::new(1, 1)), 2.into());
        // Line 0 is empty, so character 1 (past its end) clamps to the line start (byte 0).
        assert_eq!(file_db.offset(Position::new(0, 1)), 0.into());
        assert_eq!(file_db.offset(Position::new(0, 0)), 0.into());
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

    /// LSP `character` is a UTF-16 code-unit count. A multi-byte character earlier on a line must
    /// shift the byte offset correctly (the old code added `character` as a raw byte offset, which
    /// was only correct for ASCII).
    #[test]
    fn utf16_offset_round_trip_test() {
        // "é" is 2 bytes in UTF-8 / 1 UTF-16 unit; "😀" is 4 bytes / 2 UTF-16 units (surrogate pair).
        let source = "é😀x";
        let file_db = FileDB::new(
            FileId(1),
            source,
            Url::from_file_path(Path::new("/tmp.txt")).unwrap(),
        );

        // byte offsets: é=0, 😀=2, x=6. UTF-16 units: é=0, 😀=1, x=3.
        assert_eq!(
            file_db.offset(Position::new(0, 0)),
            0.into(),
            "char 0 -> byte 0"
        );
        assert_eq!(
            file_db.offset(Position::new(0, 1)),
            2.into(),
            "char 1 (after é) -> byte 2"
        );
        assert_eq!(
            file_db.offset(Position::new(0, 3)),
            6.into(),
            "char 3 (after 😀) -> byte 6"
        );
        // Round trip back.
        assert_eq!(file_db.position(6.into()), Position::new(0, 3));
        assert_eq!(file_db.position(2.into()), Position::new(0, 1));
    }
}
