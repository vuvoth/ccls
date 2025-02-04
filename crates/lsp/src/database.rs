use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use std::collections::hash_map::DefaultHasher;

use lsp_types::{Position, Range, Url};

use rowan::{ast::AstNode, TextSize};
use syntax::{
    abstract_syntax_tree::{
        AstCircomProgram, AstComponentDecl, AstInputSignalDecl, AstOutputSignalDecl, AstSignalDecl,
        AstTemplateDef, AstVarDecl,
    },
    syntax_node::{SyntaxNode, SyntaxToken},
};

/**
* We will store
* Open data -> Parse -> output -> Syntax -> analyzer -> db{
   FileID {
       Template {
           signal,

       }
   }

                               value
   Template map: { Hash(FileID, token) -> Template}
   Vars map: {Hash(FileID, template, token)} -> Var}
   Component map {Hash(FileID, template, token)} -> ComponentInfo
   Signals map {Hash(FileID, template, token)} -> Signal



}
*/

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct FileId(pub u64);

#[derive(Clone)]
pub struct FileDB {
    pub file_id: FileId,
    pub file_path: Url,
    pub end_line_vec: Vec<u32>,
}

use path_absolutize::*;

impl FileDB {
    pub fn create(content: &str, file_path: Url) -> Self {
        let mut hasher = DefaultHasher::new();
        file_path
            .to_file_path()
            .unwrap()
            .absolutize()
            .unwrap()
            .hash(&mut hasher);
        Self::new(FileId(hasher.finish()), content, file_path)
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

#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug)]
pub struct Id(pub u64);

pub trait TokenId {
    fn token_id(&self) -> Id;
}

impl TokenId for SyntaxNode {
    fn token_id(&self) -> Id {
        let mut hasher = DefaultHasher::new();
        self.to_string().hash(&mut hasher);
        Id(hasher.finish())
    }
}

impl TokenId for SyntaxToken {
    fn token_id(&self) -> Id {
        let mut hasher = DefaultHasher::new();
        self.to_string().hash(&mut hasher);
        Id(hasher.finish())
    }
}

#[derive(Debug, Clone)]
pub struct SemanticLocations(pub HashMap<Id, Vec<Range>>);

impl Default for SemanticLocations {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticLocations {
    pub fn insert(&mut self, token_id: Id, range: Range) {
        if let Some(locations) = self.0.get_mut(&token_id) {
            locations.push(range);
        } else {
            self.0.insert(token_id, vec![range]);
        }
    }
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

#[derive(Debug, Clone)]
pub struct TemplateDataSemantic {
    pub signal: SemanticLocations,
    pub variable: SemanticLocations,
    pub component: SemanticLocations,
}

impl TemplateDataSemantic {
    fn new() -> Self {
        Self {
            signal: SemanticLocations::new(),
            variable: SemanticLocations::new(),
            component: SemanticLocations::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SemanticData {
    pub template: SemanticLocations,
    pub template_data_semantic: HashMap<Id, TemplateDataSemantic>,
}

pub enum TemplateDataInfo {
    Signal((Id, Range)),
    Variable((Id, Range)),
    Component((Id, Range)),
}
pub enum SemanticInfo {
    Template((Id, Range)),
    TemplateData((Id, TemplateDataInfo)),
}

#[derive(Debug, Clone)]
pub struct SemanticDB {
    pub semantic: HashMap<FileId, SemanticData>,
}

impl Default for SemanticDB {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticDB {
    pub fn new() -> Self {
        Self {
            semantic: HashMap::new(),
        }
    }

    pub fn insert(&mut self, file_id: FileId, semantic_info: SemanticInfo) {
        let semantic = self.semantic.entry(file_id).or_insert(SemanticData {
            template: SemanticLocations::new(),
            template_data_semantic: HashMap::new(),
        });

        match semantic_info {
            SemanticInfo::Template((id, range)) => {
                semantic.template.insert(id, range);
            }
            SemanticInfo::TemplateData((template_id, template_data_info)) => {
                let template_semantic = semantic
                    .template_data_semantic
                    .entry(template_id)
                    .or_insert(TemplateDataSemantic::new());

                match template_data_info {
                    TemplateDataInfo::Component((id, r)) => {
                        template_semantic.component.insert(id, r)
                    }
                    TemplateDataInfo::Variable((id, r)) => template_semantic.variable.insert(id, r),
                    TemplateDataInfo::Signal((id, r)) => template_semantic.signal.insert(id, r),
                }
            }
        }
    }

    pub fn circom_program_semantic(
        &mut self,
        file_db: &FileDB,
        abstract_syntax_tree: &AstCircomProgram,
    ) {
        for template in abstract_syntax_tree.template_list() {
            if let Some(name) = template.name() {
                let template_id = name.syntax().token_id();
                self.insert(
                    file_db.file_id,
                    SemanticInfo::Template((template_id, file_db.range(template.syntax()))),
                );
                self.template_semantic(file_db, &template);
            }
        }
    }

    pub fn template_semantic(&mut self, file_db: &FileDB, ast_template: &AstTemplateDef) {
        let template_id = ast_template.syntax().token_id();

        if let Some(statements) = ast_template.statements() {
            for signal in statements.find_children::<AstInputSignalDecl>() {
                if let Some(name) = signal.name() {
                    self.insert(
                        file_db.file_id,
                        SemanticInfo::TemplateData((
                            template_id,
                            TemplateDataInfo::Signal((
                                name.syntax().token_id(),
                                file_db.range(signal.syntax()),
                            )),
                        )),
                    );
                }
            }
            for signal in statements.find_children::<AstOutputSignalDecl>() {
                if let Some(name) = signal.name() {
                    self.insert(
                        file_db.file_id,
                        SemanticInfo::TemplateData((
                            template_id,
                            TemplateDataInfo::Signal((
                                name.syntax().token_id(),
                                file_db.range(signal.syntax()),
                            )),
                        )),
                    );
                }
            }

            for signal in statements.find_children::<AstSignalDecl>() {
                if let Some(name) = signal.name() {
                    self.insert(
                        file_db.file_id,
                        SemanticInfo::TemplateData((
                            template_id,
                            TemplateDataInfo::Signal((
                                name.syntax().token_id(),
                                file_db.range(signal.syntax()),
                            )),
                        )),
                    );
                }
            }

            for var in statements.find_children::<AstVarDecl>() {
                if let Some(name) = var.name() {
                    self.insert(
                        file_db.file_id,
                        SemanticInfo::TemplateData((
                            template_id,
                            TemplateDataInfo::Variable((
                                name.syntax().token_id(),
                                file_db.range(var.syntax()),
                            )),
                        )),
                    );
                }
            }

            for component in statements.find_children::<AstComponentDecl>() {
                if let Some(component_var) = component.component_identifier() {
                    if let Some(name) = component_var.name() {
                        self.insert(
                            file_db.file_id,
                            SemanticInfo::TemplateData((
                                template_id,
                                TemplateDataInfo::Component((
                                    name.syntax().token_id(),
                                    file_db.range(component.syntax()),
                                )),
                            )),
                        );
                    }
                }
            }
        }
    }
}

impl SemanticData {
    pub fn lookup_signal(&self, template_id: Id, signal: &SyntaxToken) -> Option<&Vec<Range>> {
        if let Some(semantic_template) = self.template_data_semantic.get(&template_id) {
            return semantic_template.signal.0.get(&signal.token_id());
        }
        None
    }

    // TODO: remove duplicate code here.
    pub fn lookup_variable(&self, template_id: Id, variable: &SyntaxToken) -> Option<&Vec<Range>> {
        if let Some(semantic_template) = self.template_data_semantic.get(&template_id) {
            return semantic_template.variable.0.get(&variable.token_id());
        }
        None
    }

    pub fn lookup_component(
        &self,
        template_id: Id,
        component: &SyntaxToken,
    ) -> Option<&Vec<Range>> {
        if let Some(semantic_template) = self.template_data_semantic.get(&template_id) {
            return semantic_template.component.0.get(&component.token_id());
        }
        None
    }
}

#[cfg(test)]
mod tests {

    use std::path::Path;

    use ::syntax::{abstract_syntax_tree::AstCircomProgram, syntax::SyntaxTreeBuilder};
    use lsp_types::{Position, Url};

    use rowan::ast::AstNode;

    use crate::database::{FileDB, FileId};

    use super::TokenId;

    #[test]
    fn file_id_test() {
        let file_1 = FileDB::create("a", Url::from_file_path(Path::new("/a/../a/c")).unwrap());
        let file_2 = FileDB::create("a", Url::from_file_path(Path::new("/a/c")).unwrap());

        assert_eq!(file_1.file_id, file_2.file_id);
    }
    #[test]
    fn token_id_hash_test() {
        let source: String = r#"pragma circom 2.0.0;

        
        template Multiplier2 () {}
        template Multiplier2 () {} 
        "#
        .to_string();

        let syntax = SyntaxTreeBuilder::syntax_tree(&source);

        if let Some(ast) = AstCircomProgram::cast(syntax) {
            let templates = ast.template_list();
            let first_id = templates[0].syntax().token_id();
            let second_id = templates[1].syntax().token_id();

            assert_eq!(first_id, second_id);
        }
    }
    #[test]
    fn off_set_test() {
        let str = r#"
one
two
three
       "#;

        let file_utils = FileDB::new(
            FileId(1),
            str,
            Url::from_file_path(Path::new("/tmp.txt")).unwrap(),
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
        let file_utils = FileDB::new(
            FileId(1),
            str,
            Url::from_file_path(Path::new("/tmp.txt")).unwrap(),
        );
        assert_eq!(Position::new(1, 1), file_utils.position(2.into()));
        assert_eq!(Position::new(0, 0), file_utils.position(0.into()));
    }
}
