use std::collections::HashMap;

use lsp_types::Position;

pub struct FileId(u32);

pub struct FileUtils {
    file_id: FileId,
    end_line_vec: Vec<u32>,
}

impl FileUtils {
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

    pub fn off_set(&self, position: Position) -> Option<u32> {
        if position.line < self.end_line_vec.len() as u32 {
            return Some(self.end_line_vec[position.line as usize] + position.character)
        }
        None
    }
}

