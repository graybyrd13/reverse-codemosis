//! A project held as one connected structure: every source file, parsed, with
//! per-file analyses cached so facts can be asked cheaply and across files.

use crate::lang::{self, Language};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;
use tree_sitter::{Parser, Tree};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("the {language} parser could not parse {path}")]
    Parse { path: PathBuf, language: String },
    #[error("{0} is not a directory")]
    NotADirectory(PathBuf),
}

/// One parsed source file.
pub struct SourceFile {
    /// Path relative to the project root, using `/` separators.
    pub path: PathBuf,
    pub lang: &'static dyn Language,
    pub text: String,
    pub tree: Tree,
    analysis: OnceLock<Box<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceFile")
            .field("path", &self.path)
            .field("lang", &self.lang.id())
            .finish()
    }
}

impl SourceFile {
    pub fn parse(
        path: impl Into<PathBuf>,
        lang: &'static dyn Language,
        text: String,
    ) -> Result<SourceFile, ProjectError> {
        let path = path.into();
        let tree = parse_text(lang, &text).ok_or_else(|| ProjectError::Parse {
            path: path.clone(),
            language: lang.id().to_string(),
        })?;
        Ok(SourceFile {
            path,
            lang,
            text,
            tree,
            analysis: OnceLock::new(),
        })
    }

    /// The language-specific analysis of this file, computed on first use.
    ///
    /// Each language stores exactly one analysis type; asking for another type
    /// is a programming error.
    pub fn analysis<T: Any + Send + Sync>(&self, compute: impl FnOnce(&SourceFile) -> T) -> &T {
        self.analysis
            .get_or_init(|| Box::new(compute(self)))
            .downcast_ref::<T>()
            .expect("one analysis type per language")
    }

    /// Number of syntax errors (ERROR and MISSING nodes) in the tree.
    pub fn syntax_errors(&self) -> usize {
        count_errors(self.tree.root_node())
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    pub fn display_path(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }
}

fn count_errors(node: tree_sitter::Node) -> usize {
    if !node.has_error() {
        return 0;
    }
    let mut n = usize::from(node.is_error() || node.is_missing());
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        n += count_errors(child);
    }
    n
}

pub fn parse_text(lang: &dyn Language, text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&lang.grammar()).ok()?;
    parser.parse(text, None)
}

/// Which files belong to a project.
#[derive(Clone, Debug, Default)]
pub struct FileFilter {
    /// Only files under these relative paths (empty means everything).
    pub include: Vec<PathBuf>,
    /// Relative paths to skip, in addition to the defaults and `.gitignore`.
    pub exclude: Vec<PathBuf>,
}

const ALWAYS_SKIPPED: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".codemosis",
];

/// Every source file of a project, parsed.
#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub files: Vec<SourceFile>,
}

impl Project {
    /// Load every file under `root` that a known language can read,
    /// honouring `.gitignore`.
    pub fn load(root: &Path, filter: &FileFilter) -> Result<Project, ProjectError> {
        if !root.is_dir() {
            return Err(ProjectError::NotADirectory(root.to_path_buf()));
        }
        let mut paths = Vec::new();
        let walker = ignore::WalkBuilder::new(root)
            .hidden(true)
            .git_ignore(true)
            .require_git(false)
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !(e.depth() > 0 && ALWAYS_SKIPPED.contains(&name.as_ref()))
            })
            .build();
        for entry in walker.flatten() {
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let abs = entry.path();
            let rel = abs.strip_prefix(root).unwrap_or(abs).to_path_buf();
            if !filter.include.is_empty() && !filter.include.iter().any(|p| rel.starts_with(p)) {
                continue;
            }
            if filter.exclude.iter().any(|p| rel.starts_with(p)) {
                continue;
            }
            if lang::for_path(&rel).is_some() {
                paths.push(rel);
            }
        }
        paths.sort();
        let mut files = Vec::with_capacity(paths.len());
        for rel in paths {
            let abs = root.join(&rel);
            let text = std::fs::read_to_string(&abs).map_err(|source| ProjectError::Read {
                path: abs.clone(),
                source,
            })?;
            let lang = lang::for_path(&rel).expect("checked above");
            files.push(SourceFile::parse(normalize(&rel), lang, text)?);
        }
        Ok(Project {
            root: root.to_path_buf(),
            files,
        })
    }

    /// A project that exists only in memory (used by pack self-tests).
    pub fn in_memory(files: Vec<SourceFile>) -> Project {
        Project {
            root: PathBuf::from("<memory>"),
            files,
        }
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.files.iter().position(|f| f.path == path)
    }

    /// Replace a file's text and reparse it.
    pub fn set_text(&mut self, index: usize, text: String) -> Result<(), ProjectError> {
        let old = &self.files[index];
        let file = SourceFile::parse(old.path.clone(), old.lang, text)?;
        self.files[index] = file;
        Ok(())
    }

    pub fn languages(&self) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = self.files.iter().map(|f| f.lang.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

fn normalize(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().replace('\\', "/"))
}
