#[derive(Debug, Clone)]
pub struct VirtualFile {
    content: String,
}

impl VirtualFile {
    pub fn new(content: String) -> Self {
        Self {
            content,
        }
    }

    pub fn get_file_content(&self) -> String {
        self.content.clone()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct FilePath {
    pub path: String
}

impl FilePath {
    pub fn new(path: String) -> Self {
        FilePath { path }
    }
}
