use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
};

use lsp_types::{Position, Range, Url};
use path_absolutize::*;
use rowan::TextSize;
use syntax::{
    abstract_syntax_tree::{Function, Program, Template},
    syntax_node::SyntaxNode,
};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct FileId(pub u64);

#[derive(Clone)]
pub struct FileDB {
    pub id: FileId,
    pub url: Url,
    line_ends: Vec<u32>,
}

impl FileDB {
    pub fn new(content: &str, url: Url) -> Self {
        let id = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            if let Ok(p) = url.to_file_path() {
                if let Ok(abs) = p.absolutize() {
                    abs.to_path_buf().hash(&mut hasher);
                }
            }
            FileId(hasher.finish())
        };

        let line_ends = content
            .char_indices()
            .filter_map(|(i, c)| (c == '\n').then_some(i as u32))
            .collect();

        Self { id, url, line_ends }
    }

    pub fn path(&self) -> PathBuf {
        self.url.to_file_path().unwrap_or_default()
    }

    pub fn offset(&self, pos: Position) -> TextSize {
        if pos.line == 0 {
            return pos.character.into();
        }
        self.line_ends
            .get(pos.line as usize - 1)
            .map(|&end| (end + pos.character + 1).into())
            .unwrap_or_else(|| self.line_ends.last().copied().unwrap_or(0).into())
    }

    pub fn position(&self, offset: TextSize) -> Position {
        let offset: u32 = offset.into();
        let line = self.line_ends.binary_search(&offset).unwrap_or_else(|x| x);
        let char = if line > 0 {
            offset.saturating_sub(self.line_ends.get(line - 1).copied().unwrap_or(0) + 1)
        } else {
            offset
        };
        Position::new(line as u32, char)
    }

    pub fn range(&self, node: &SyntaxNode) -> Range {
        let r = node.text_range();
        Range::new(self.position(r.start()), self.position(r.end()))
    }
}

#[derive(Debug, Clone)]
pub struct Def {
    pub range: Range,
    pub kind: DefKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Template,
    Function,
    Param,
    Signal,
    Var,
    Component,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolTable(HashMap<String, Def>);

impl SymbolTable {
    pub fn insert(&mut self, name: impl Into<String>, def: Def) {
        self.0.insert(name.into(), def);
    }

    pub fn get(&self, name: &str) -> Option<&Def> {
        self.0.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScopeSymbols {
    pub params: SymbolTable,
    pub signals: SymbolTable,
    pub vars: SymbolTable,
    pub components: SymbolTable,
}

impl ScopeSymbols {
    pub fn lookup(&self, name: &str) -> Option<&Def> {
        self.params
            .get(name)
            .or_else(|| self.signals.get(name))
            .or_else(|| self.vars.get(name))
            .or_else(|| self.components.get(name))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.params.contains(name)
            || self.signals.contains(name)
            || self.vars.contains(name)
            || self.components.contains(name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileSemantics {
    pub templates: SymbolTable,
    pub functions: SymbolTable,
    pub template_scopes: HashMap<String, ScopeSymbols>,
    pub function_scopes: HashMap<String, ScopeSymbols>,
}

impl FileSemantics {
    pub fn lookup_global(&self, name: &str) -> Option<&Def> {
        self.templates
            .get(name)
            .or_else(|| self.functions.get(name))
    }

    pub fn lookup_in_template(&self, template_name: &str, symbol_name: &str) -> Option<&Def> {
        self.template_scopes.get(template_name)?.lookup(symbol_name)
    }

    pub fn lookup_in_function(&self, function_name: &str, symbol_name: &str) -> Option<&Def> {
        self.function_scopes.get(function_name)?.lookup(symbol_name)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticDB {
    pub files: HashMap<FileId, FileSemantics>,
}

impl SemanticDB {
    pub fn get(&self, id: FileId) -> Option<&FileSemantics> {
        self.files.get(&id)
    }

    pub fn get_mut(&mut self, id: FileId) -> &mut FileSemantics {
        self.files.entry(id).or_default()
    }

    pub fn index_program(&mut self, file: &FileDB, program: &Program) {
        let semantics = self.files.entry(file.id).or_default();

        for template in program.templates() {
            if let Some(name) = template.name() {
                let name_str = name.text();
                semantics.templates.insert(
                    name_str,
                    Def {
                        range: file.range(template.syntax_node()),
                        kind: DefKind::Template,
                    },
                );

                let scope = semantics
                    .template_scopes
                    .entry(name_str.to_string())
                    .or_default();
                Self::index_template(scope, file, &template);
            }
        }

        for function in program.functions() {
            if let Some(name) = function.name() {
                let name_str = name.text();
                semantics.functions.insert(
                    name_str,
                    Def {
                        range: file.range(function.syntax_node()),
                        kind: DefKind::Function,
                    },
                );

                let scope = semantics
                    .function_scopes
                    .entry(name_str.to_string())
                    .or_default();
                Self::index_function(scope, file, &function);
            }
        }
    }

    fn index_template(scope: &mut ScopeSymbols, file: &FileDB, template: &Template) {
        if let Some(params) = template.params() {
            for param in params.idents() {
                let name = param.text();
                scope.params.insert(
                    name,
                    Def {
                        range: file.range(&param.syntax_token().parent().unwrap()),
                        kind: DefKind::Param,
                    },
                );
            }
        }

        for signal in template.signals() {
            if let Some(ident) = signal.ident() {
                let name = ident.text();
                scope.signals.insert(
                    name,
                    Def {
                        range: file.range(signal.syntax_node()),
                        kind: DefKind::Signal,
                    },
                );
            }
        }

        for var in template.vars() {
            if let Some(ident) = var.ident() {
                let name = ident.text();
                scope.vars.insert(
                    name,
                    Def {
                        range: file.range(var.syntax_node()),
                        kind: DefKind::Var,
                    },
                );
            }
        }

        for comp in template.components() {
            if let Some(ident) = comp.ident() {
                let name = ident.text();
                scope.components.insert(
                    name,
                    Def {
                        range: file.range(comp.syntax_node()),
                        kind: DefKind::Component,
                    },
                );
            }
        }
    }

    fn index_function(scope: &mut ScopeSymbols, file: &FileDB, function: &Function) {
        if let Some(params) = function.params() {
            for param in params.idents() {
                let name = param.text();
                scope.params.insert(
                    name,
                    Def {
                        range: file.range(&param.syntax_token().parent().unwrap()),
                        kind: DefKind::Param,
                    },
                );
            }
        }

        for var in function.vars() {
            if let Some(ident) = var.ident() {
                let name = ident.text();
                scope.vars.insert(
                    name,
                    Def {
                        range: file.range(var.syntax_node()),
                        kind: DefKind::Var,
                    },
                );
            }
        }

        for comp in function.components() {
            if let Some(ident) = comp.ident() {
                let name = ident.text();
                scope.components.insert(
                    name,
                    Def {
                        range: file.range(comp.syntax_node()),
                        kind: DefKind::Component,
                    },
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lsp_types::{Position, Url};
    use rowan::ast::AstNode;
    use syntax::{abstract_syntax_tree::Program, syntax::SyntaxTreeBuilder};

    use super::{DefKind, FileDB};

    #[test]
    fn file_id_test() {
        let file_1 = FileDB::new("a", Url::from_file_path(Path::new("/a/../a/c")).unwrap());
        let file_2 = FileDB::new("a", Url::from_file_path(Path::new("/a/c")).unwrap());
        assert_eq!(file_1.id, file_2.id);
    }

    #[test]
    fn test_symbol_resolution() {
        let source = r#"
pragma circom 2.0.0;

template Multiplier() {
    signal input a;
    signal input b;
    signal output c;
    c <== a * b;
}

template Main() {
    signal input x;
    component mult = Multiplier();
}
"#;
        let url = Url::from_file_path(Path::new("/test.circom")).unwrap();
        let file = FileDB::new(source, url);
        let syntax = SyntaxTreeBuilder::syntax_tree(source);

        if let Some(program) = Program::cast(syntax) {
            let mut db = super::SemanticDB::default();
            db.index_program(&file, &program);

            let semantics = db.get(file.id).unwrap();

            assert!(semantics.templates.get("Multiplier").is_some());
            assert_eq!(
                semantics.templates.get("Multiplier").unwrap().kind,
                DefKind::Template
            );

            let signal_def = semantics.lookup_in_template("Multiplier", "a");
            assert!(signal_def.is_some());
            assert_eq!(signal_def.unwrap().kind, DefKind::Signal);

            let comp_def = semantics.lookup_in_template("Main", "mult");
            assert!(comp_def.is_some());
            assert_eq!(comp_def.unwrap().kind, DefKind::Component);
        }
    }

    #[test]
    fn off_set_test() {
        let str = "\none\ntwo\nthree";
        let file = FileDB::new(str, Url::from_file_path(Path::new("/tmp.txt")).unwrap());

        assert_eq!(file.offset(Position::new(0, 1)), 1.into());
        assert_eq!(file.offset(Position::new(1, 1)), 2.into());
    }

    #[test]
    fn position_test() {
        let str = "\none\ntwo\nthree";
        let file = FileDB::new(str, Url::from_file_path(Path::new("/tmp.txt")).unwrap());

        assert_eq!(file.position(0.into()), Position::new(0, 0));
        assert_eq!(file.position(2.into()), Position::new(1, 1));
    }
}
