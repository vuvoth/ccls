use lsp_types::Position;
use rowan::TextSize;

pub struct FileId(u32);

pub struct FileUtils {
    file_id: FileId,
    end_line_vec: Vec<u32>,
}

impl FileUtils {
    pub fn create(content: &str) -> Self {
        Self::new(FileId(0), content)
    }

    pub fn new(file_id: FileId, content: &str) -> Self {
        let mut file_utils = Self {
            file_id,
            end_line_vec: Vec::new(),
        };

        let mut id = 0;
        for c in content.chars() {
            if c == '\n' {
                file_utils.end_line_vec.push(id);
            }
            id += 1;
        }

        file_utils
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
}

#[cfg(test)]
mod tests {
    use lsp_types::Position;

    use crate::utils::{FileId, FileUtils};

    #[test]
    fn off_set_test() {
        let str = r#"
one
two
three
       "#;

        let file_utils = FileUtils::new(FileId(1), str);

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
        let file_utils = FileUtils::new(FileId(1), str);
        assert_eq!(Position::new(1, 1), file_utils.position(2.into()));
        assert_eq!(Position::new(0, 0), file_utils.position(0.into()));
    }
}
