use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u64);

impl From<FileId> for u64 {
    fn from(value: FileId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VfsPath(Arc<PathBuf>);

impl VfsPath {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        let normalized = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        };
        Self(Arc::new(normalized))
    }

    pub fn from_url(url: &url::Url) -> Option<Self> {
        url.to_file_path().ok().map(Self::new)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn parent(&self) -> Option<Self> {
        self.0.parent().map(|p| Self(Arc::new(p.to_path_buf())))
    }

    pub fn join<P: AsRef<Path>>(&self, other: P) -> Self {
        Self(Arc::new(self.0.join(other)))
    }

    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name().and_then(|n| n.to_str())
    }

    pub fn extension(&self) -> Option<&str> {
        self.0.extension().and_then(|e| e.to_str())
    }

    pub fn file_id(&self) -> FileId {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.0.hash(&mut hasher);
        FileId(hasher.finish())
    }
}

impl std::fmt::Display for VfsPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl AsRef<Path> for VfsPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct VirtualFile {
    pub path: VfsPath,
    pub content: String,
    pub version: i32,
    pub line_ends: Vec<u32>,
}

impl VirtualFile {
    pub fn new(path: VfsPath, content: String) -> Self {
        let line_ends = content
            .char_indices()
            .filter_map(|(i, c)| if c == '\n' { Some(i as u32) } else { None })
            .collect();

        Self {
            path,
            content,
            version: 0,
            line_ends,
        }
    }

    pub fn update(&mut self, content: String) {
        let line_ends = content
            .char_indices()
            .filter_map(|(i, c)| if c == '\n' { Some(i as u32) } else { None })
            .collect();
        self.content = content;
        self.line_ends = line_ends;
        self.version += 1;
    }

    pub fn file_id(&self) -> FileId {
        self.path.file_id()
    }

    pub fn offset(&self, position: lsp_types::Position) -> rowan::TextSize {
        if position.line == 0 {
            return position.character.into();
        }
        let line_idx = (position.line as usize).saturating_sub(1);
        if line_idx < self.line_ends.len() {
            (self.line_ends[line_idx] + position.character + 1).into()
        } else {
            (self.content.len() as u32).into()
        }
    }

    pub fn position(&self, offset: rowan::TextSize) -> lsp_types::Position {
        let offset = u32::from(offset) as usize;
        let line = self
            .line_ends
            .binary_search(&(offset as u32))
            .unwrap_or_else(|x| x);

        lsp_types::Position::new(
            line as u32,
            if line > 0 && line <= self.line_ends.len() {
                (offset as u32).saturating_sub(self.line_ends[line - 1] + 1)
            } else {
                offset as u32
            },
        )
    }

    pub fn range(&self, range: &rowan::TextRange) -> lsp_types::Range {
        lsp_types::Range {
            start: self.position(range.start()),
            end: self.position(range.end()),
        }
    }
}

#[derive(Debug, Default)]
pub struct Vfs {
    files: HashMap<FileId, VirtualFile>,
    paths: HashMap<VfsPath, FileId>,
}

impl Vfs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: VfsPath, content: String) -> FileId {
        let file_id = path.file_id();

        if let Some(existing) = self.files.get_mut(&file_id) {
            existing.update(content);
        } else {
            let file = VirtualFile::new(path.clone(), content);
            self.files.insert(file_id, file);
            self.paths.insert(path, file_id);
        }

        file_id
    }

    pub fn remove(&mut self, path: &VfsPath) -> Option<VirtualFile> {
        let file_id = path.file_id();
        self.paths.remove(path);
        self.files.remove(&file_id)
    }

    pub fn get(&self, path: &VfsPath) -> Option<&VirtualFile> {
        self.paths.get(path).and_then(|id| self.files.get(id))
    }

    pub fn get_by_id(&self, id: FileId) -> Option<&VirtualFile> {
        self.files.get(&id)
    }

    pub fn get_mut(&mut self, path: &VfsPath) -> Option<&mut VirtualFile> {
        self.paths.get(path).and_then(|id| self.files.get_mut(id))
    }

    pub fn contains(&self, path: &VfsPath) -> bool {
        self.paths.contains_key(path)
    }

    pub fn file_ids(&self) -> impl Iterator<Item = FileId> + '_ {
        self.files.keys().copied()
    }

    pub fn paths(&self) -> impl Iterator<Item = &VfsPath> {
        self.paths.keys()
    }

    pub fn load(&mut self, path: VfsPath) -> std::io::Result<FileId> {
        let content = std::fs::read_to_string(path.as_path())?;
        Ok(self.insert(path, content))
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_path() {
        let path = VfsPath::new("/test/file.circom");
        assert_eq!(path.file_name(), Some("file.circom"));
        assert_eq!(path.extension(), Some("circom"));
    }

    #[test]
    fn test_vfs_insert() {
        let mut vfs = Vfs::new();
        let path = VfsPath::new("/test/file.circom");
        let id = vfs.insert(path.clone(), "content".to_string());

        assert!(vfs.contains(&path));
        assert_eq!(vfs.get(&path).unwrap().content, "content");
        assert_eq!(vfs.get_by_id(id).unwrap().content, "content");
    }

    #[test]
    fn test_vfs_update() {
        let mut vfs = Vfs::new();
        let path = VfsPath::new("/test/file.circom");
        let id = vfs.insert(path.clone(), "content".to_string());

        vfs.insert(path.clone(), "new content".to_string());

        let file = vfs.get_by_id(id).unwrap();
        assert_eq!(file.content, "new content");
        assert_eq!(file.version, 1);
    }

    #[test]
    fn test_position_conversion() {
        let path = VfsPath::new("/test.circom");
        let file = VirtualFile::new(path, "line1\nline2\nline3".to_string());

        let pos = file.position(0.into());
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        let pos = file.position(6.into());
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }
}
