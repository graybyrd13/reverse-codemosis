//! A project held as one connected structure: every text file, identified
//! by GitHub's catalogue of languages, parsed into a syntax tree where a
//! grammar exists, segmented into code, comments and strings where the
//! language's lexical family is known, and with per-file analyses cached so
//! facts can be asked cheaply and across files.

use crate::lang::{self, Language};
use crate::lex::{self, Profile, SegKind, Segment};
use crate::linguist::{self, Confidence, Detection, LinguistLanguage, Role};
use std::any::Any;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;
use tree_sitter::{Node, Parser, Tree};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("{0} is not a directory")]
    NotADirectory(PathBuf),
}

/// How deeply the engine understands a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// The language is known, but not its comment and string syntax.
    Recognized,
    /// Comments and strings can be told from code.
    Lexical,
    /// A full syntax tree, and the language's own facts.
    Deep,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Recognized => "recognized",
            Tier::Lexical => "lexical",
            Tier::Deep => "deep",
        }
    }
}

/// One text file of the project.
pub struct SourceFile {
    /// Path relative to the project root, using `/` separators.
    pub path: PathBuf,
    pub text: String,
    pub detection: Detection,
    pub role: Role,
    /// The language with a grammar, when the file has one and its language
    /// was detected with confidence.
    pub lang: Option<&'static dyn Language>,
    pub tree: Option<Tree>,
    pub lexical: Option<&'static Profile>,
    analysis: OnceLock<Box<dyn Any + Send + Sync>>,
    segments: OnceLock<Vec<Segment>>,
    /// Per-file memo for facts that would otherwise repeat work per match.
    memo: std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<dyn Any + Send + Sync>>>,
}

impl std::fmt::Debug for SourceFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceFile")
            .field("path", &self.path)
            .field("language", &self.language_name())
            .field("tier", &self.tier())
            .finish()
    }
}

impl SourceFile {
    /// Identify, classify and parse a file.
    pub fn new(path: impl Into<PathBuf>, text: String) -> SourceFile {
        let path = path.into();
        let detection = linguist::detect(&path, &text);
        let role = linguist::role(&path, &text);
        let lang = if detection.trusted() {
            detection.language.and_then(|l| lang::for_linguist(l.name))
        } else {
            None
        };
        SourceFile::build(path, text, detection, role, lang)
    }

    /// A file parsed as a given language, regardless of its name.
    pub fn with_language(path: impl Into<PathBuf>, lang: &'static dyn Language, text: String) -> SourceFile {
        let path = path.into();
        let mut detection = linguist::detect(&path, &text);
        if let Some(l) = lang.linguist_names().first().and_then(|n| linguist::by_name(n)) {
            detection.language = Some(l);
            detection.confidence = Confidence::Certain;
        }
        let role = linguist::role(&path, &text);
        SourceFile::build(path, text, detection, role, Some(lang))
    }

    fn build(
        path: PathBuf,
        text: String,
        detection: Detection,
        role: Role,
        lang: Option<&'static dyn Language>,
    ) -> SourceFile {
        let tree = lang.and_then(|l| parse_text(l, &text));
        let lang = if tree.is_some() { lang } else { None };
        let lexical = if detection.trusted() {
            detection.language.and_then(lex::profile_for)
        } else {
            None
        };
        SourceFile {
            path,
            text,
            detection,
            role,
            lang,
            tree,
            lexical,
            analysis: OnceLock::new(),
            segments: OnceLock::new(),
            memo: Default::default(),
        }
    }

    /// The same file with new text: same identity, reparsed.
    pub fn with_text(&self, text: String) -> SourceFile {
        SourceFile::build(
            self.path.clone(),
            text,
            self.detection.clone(),
            self.role.clone(),
            self.lang,
        )
    }

    pub fn linguist(&self) -> Option<&'static LinguistLanguage> {
        self.detection.language
    }

    pub fn language_name(&self) -> &'static str {
        self.detection.language.map(|l| l.name).unwrap_or("Unknown")
    }

    /// What the file is for, from the conventions of its path.
    pub fn purpose(&self) -> Purpose {
        purpose_of(&self.path)
    }

    pub fn tier(&self) -> Tier {
        if self.tree.is_some() {
            Tier::Deep
        } else if self.lexical.is_some() {
            Tier::Lexical
        } else {
            Tier::Recognized
        }
    }

    pub fn root(&self) -> Option<Node<'_>> {
        self.tree.as_ref().map(|t| t.root_node())
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

    /// A value computed once per file and key, then shared: for facts whose
    /// naive form would redo the same work for every match.
    pub fn memo<T: Any + Send + Sync>(&self, key: String, compute: impl FnOnce() -> T) -> std::sync::Arc<T> {
        if let Some(v) = self.memo.lock().expect("memo lock").get(&key) {
            if let Ok(t) = v.clone().downcast::<T>() {
                return t;
            }
        }
        let value = std::sync::Arc::new(compute());
        self.memo
            .lock()
            .expect("memo lock")
            .insert(key, value.clone() as std::sync::Arc<dyn Any + Send + Sync>);
        value
    }

    /// The file split into code, comments and strings. Empty when the
    /// engine cannot tell them apart in this language.
    pub fn segments(&self) -> &[Segment] {
        self.segments.get_or_init(|| {
            if let Some(root) = self.root() {
                tree_segments(self, root)
            } else if let Some(p) = self.lexical {
                lex::segments(&self.text, p)
            } else {
                Vec::new()
            }
        })
    }

    /// True when code, comments and strings can be told apart.
    pub fn knows_comments(&self) -> bool {
        self.tree.is_some() || self.lexical.is_some()
    }

    /// The kind of text at an offset, if it can be known.
    pub fn kind_at(&self, offset: usize) -> Option<SegKind> {
        self.knows_comments().then(|| lex::kind_at(self.segments(), offset))
    }

    /// Number of syntax errors (ERROR and MISSING nodes) in the tree.
    pub fn syntax_errors(&self) -> usize {
        self.root().map(count_errors).unwrap_or(0)
    }

    /// The byte ranges whose structure the grammar could not establish: for
    /// every ERROR or MISSING node, the construct that contains it (a
    /// function with a broken header, say), or the node itself at the top
    /// level of the file. In order.
    pub fn parse_damage(&self) -> Vec<(usize, usize)> {
        fn walk(n: tree_sitter::Node, out: &mut Vec<(usize, usize)>) {
            if !n.has_error() {
                return;
            }
            if n.is_error() || n.is_missing() {
                // Climb to the construct around the damage (a missing `)`
                // sits in a parameter list, inside a function definition),
                // but never to the whole file.
                let mut region = n;
                for _ in 0..2 {
                    match region.parent() {
                        Some(p) if p.parent().is_some() && !CONTAINERS.contains(&p.kind()) => region = p,
                        _ => break,
                    }
                }
                out.push((region.start_byte(), region.end_byte()));
                return;
            }
            let mut c = n.walk();
            for child in n.children(&mut c) {
                walk(child, out);
            }
        }
        let mut out = Vec::new();
        if let Some(root) = self.root() {
            walk(root, &mut out);
        }
        out
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

    /// 1-based line of a byte offset.
    pub fn line_of(&self, offset: usize) -> usize {
        self.text[..offset.min(self.text.len())].matches('\n').count() + 1
    }
}

/// Comments and strings from a syntax tree.
fn tree_segments(file: &SourceFile, root: Node) -> Vec<Segment> {
    let lang = file.lang.expect("trees come with a language");
    let mut marks: Vec<Segment> = Vec::new();
    let mut stack = vec![root];
    let mut cursor = root.walk();
    while let Some(n) = stack.pop() {
        let kind = if lang.is_comment(&n) {
            Some(SegKind::Comment)
        } else if lang.is_string(&n) {
            Some(SegKind::String)
        } else {
            None
        };
        if let Some(k) = kind {
            marks.push(Segment {
                start: n.start_byte(),
                end: n.end_byte(),
                kind: k,
            });
            continue;
        }
        stack.extend(n.children(&mut cursor));
    }
    marks.sort_by_key(|s| s.start);
    let mut out = Vec::new();
    let mut pos = 0;
    for m in marks {
        if m.start < pos {
            continue;
        }
        if m.start > pos {
            out.push(Segment {
                start: pos,
                end: m.start,
                kind: SegKind::Code,
            });
        }
        pos = m.end;
        out.push(m);
    }
    if pos < file.text.len() {
        out.push(Segment {
            start: pos,
            end: file.text.len(),
            kind: SegKind::Code,
        });
    }
    out
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

/// Whether two texts have the same syntax tree: the same nodes in the same
/// structure with the same tokens, differing at most in the whitespace
/// between tokens. A change that keeps the tree is layout only. Both must
/// parse without errors in `lang`.
pub fn same_syntax(lang: &dyn Language, a: &str, b: &str) -> bool {
    let (Some(ta), Some(tb)) = (parse_text(lang, a), parse_text(lang, b)) else {
        return false;
    };
    let (ra, rb) = (ta.root_node(), tb.root_node());
    if ra.has_error() || rb.has_error() {
        return false;
    }
    let (mut ca, mut cb) = (ra.walk(), rb.walk());
    loop {
        let (na, nb) = (ca.node(), cb.node());
        if na.kind_id() != nb.kind_id() || na.child_count() != nb.child_count() {
            return false;
        }
        if na.child_count() == 0 && a.as_bytes()[na.byte_range()] != b.as_bytes()[nb.byte_range()] {
            return false;
        }
        // Depth-first, both cursors in step (same structure so far).
        if ca.goto_first_child() {
            cb.goto_first_child();
            continue;
        }
        loop {
            if ca.goto_next_sibling() {
                cb.goto_next_sibling();
                break;
            }
            if !ca.goto_parent() {
                return true;
            }
            cb.goto_parent();
        }
    }
}

pub fn parse_text(lang: &dyn Language, text: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser.set_language(&lang.grammar()).ok()?;
    parser.parse(text, None)
}

/// What a file is for. Rules use it to stay quiet about code whose
/// behaviour is intentional where it lives: console output in an example,
/// a pause in a demonstration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Purpose {
    Library,
    Test,
    Example,
    Script,
    Docs,
    Benchmark,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Library => "library",
            Purpose::Test => "test",
            Purpose::Example => "example",
            Purpose::Script => "script",
            Purpose::Docs => "docs",
            Purpose::Benchmark => "benchmark",
        }
    }
}

/// Build and task definitions: tooling, whatever directory they sit in.
fn is_build_file(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "gulpfile.js",
        "gulpfile.mjs",
        "gulpfile.ts",
        "gruntfile.js",
        "jakefile.js",
        "setup.py",
        "noxfile.py",
        "fabfile.py",
        "tasks.py",
        "dodo.py",
        "manage.py",
        "rakefile",
        "build.rs",
        "makefile",
        "justfile",
    ];
    NAMES.contains(&name)
        || [".config.js", ".config.mjs", ".config.cjs", ".config.ts", ".config.mts"]
            .iter()
            .any(|s| name.ends_with(s))
}

/// A file's purpose from path conventions shared across ecosystems.
pub fn purpose_of(path: &Path) -> Purpose {
    let lower = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let parts: Vec<&str> = lower.split('/').collect();
    let (dirs, name) = parts.split_at(parts.len().saturating_sub(1));
    let name = name.first().copied().unwrap_or("");
    let any_dir = |set: &[&str]| dirs.iter().any(|d| set.contains(d));
    let stem = name.split('.').next().unwrap_or(name);
    let test_name = name.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with("_tests")
        || stem.ends_with("_spec")
        || name.contains(".test.")
        || name.contains(".spec.")
        || name == "conftest.py"
        || (stem.ends_with("test") && stem.len() > 4 && path.to_string_lossy().contains("Test"));
    // Gradle and Kotlin Multiplatform source sets: src/test, src/jvmTest...
    let test_source_set = dirs
        .windows(2)
        .any(|w| w[0] == "src" && (w[1] == "test" || w[1].ends_with("test")));
    if test_name
        || test_source_set
        || any_dir(&[
            "test",
            "tests",
            "__tests__",
            "spec",
            "specs",
            "testing",
            "integration-tests",
            "e2e",
            "testdata",
            "fixtures",
            "__fixtures__",
            "__snapshots__",
        ])
    {
        Purpose::Test
    } else if any_dir(&[
        "example",
        "examples",
        "sample",
        "samples",
        "demo",
        "demos",
        "sandbox",
        "playground",
    ]) {
        Purpose::Example
    } else if any_dir(&["bench", "benches", "benchmark", "benchmarks"]) {
        Purpose::Benchmark
    } else if any_dir(&["doc", "docs", "documentation"]) {
        Purpose::Docs
    } else if any_dir(&[
        "scripts",
        "script",
        "bin",
        "tools",
        "hack",
        "ci",
        ".github",
        ".circleci",
    ]) || is_build_file(name)
    {
        Purpose::Script
    } else {
        Purpose::Library
    }
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
    ".hg",
    ".svn",
];

/// Node kinds that hold many independent statements or declarations: parse
/// damage inside one never spreads to the whole container (a header's
/// include guard, a class body, a block).
const CONTAINERS: &[&str] = &[
    "translation_unit",
    "preproc_ifdef",
    "preproc_if",
    "preproc_else",
    "preproc_elif",
    "declaration_list",
    "field_declaration_list",
    "compound_statement",
    "block",
    "statement_block",
    "class_body",
    "enum_body",
    "interface_body",
    "program",
    "module",
    "source_file",
    "statements",
    "template_body",
    "namespace_definition",
    "body_statement",
];

/// Files larger than this are not read: they are data, not code someone
/// writes by hand.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Every text file of a project.
#[derive(Debug)]
pub struct Project {
    pub root: PathBuf,
    pub files: Vec<SourceFile>,
    /// Files that were seen but not read, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
}

impl Project {
    /// Load every text file under `root`, honouring `.gitignore`.
    pub fn load(root: &Path, filter: &FileFilter) -> Result<Project, ProjectError> {
        if !root.is_dir() {
            return Err(ProjectError::NotADirectory(root.to_path_buf()));
        }
        let mut paths = Vec::new();
        let walker = ignore::WalkBuilder::new(root)
            .hidden(false)
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
            paths.push(rel);
        }
        paths.sort();
        enum Loaded {
            File(Box<SourceFile>),
            Skipped(PathBuf, String),
        }
        let loaded = crate::parallel::map(&paths, |rel| -> Result<Loaded, ProjectError> {
            let abs = root.join(rel);
            let size = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
            if size > MAX_FILE_BYTES {
                return Ok(Loaded::Skipped(
                    rel.clone(),
                    format!("larger than {} MB", MAX_FILE_BYTES / 1024 / 1024),
                ));
            }
            let bytes = std::fs::read(&abs).map_err(|source| ProjectError::Read {
                path: abs.clone(),
                source,
            })?;
            if bytes[..bytes.len().min(8000)].contains(&0) {
                return Ok(Loaded::Skipped(rel.clone(), "binary".to_string()));
            }
            Ok(match String::from_utf8(bytes) {
                Ok(text) => Loaded::File(Box::new(SourceFile::new(normalize(rel), text))),
                Err(_) => Loaded::Skipped(rel.clone(), "not UTF-8 text".to_string()),
            })
        });
        let mut files = Vec::with_capacity(paths.len());
        let mut skipped = Vec::new();
        for l in loaded {
            match l? {
                Loaded::File(f) => files.push(*f),
                Loaded::Skipped(p, why) => skipped.push((p, why)),
            }
        }
        Ok(Project {
            root: root.to_path_buf(),
            files,
            skipped,
        })
    }

    /// A project that exists only in memory (used by pack self-tests).
    pub fn in_memory(files: Vec<SourceFile>) -> Project {
        Project {
            root: PathBuf::from("<memory>"),
            files,
            skipped: Vec::new(),
        }
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.files.iter().position(|f| f.path == path)
    }

    /// Replace a file's text and reparse it.
    pub fn set_text(&mut self, index: usize, text: String) {
        let file = self.files[index].with_text(text);
        self.files[index] = file;
    }

    /// Ids of the deep languages present.
    pub fn languages(&self) -> Vec<&'static str> {
        let mut ids: Vec<&'static str> = self.files.iter().filter_map(|f| f.lang.map(|l| l.id())).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

fn normalize(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod purpose_tests {
    use super::*;

    #[test]
    fn purposes() {
        let p = |s: &str| purpose_of(Path::new(s));
        assert_eq!(p("src/requests/utils.py"), Purpose::Library);
        assert_eq!(p("tests/test_utils.py"), Purpose::Test);
        assert_eq!(p("pkg/handler_test.go"), Purpose::Test);
        assert_eq!(p("src/cart.test.js"), Purpose::Test);
        assert_eq!(p("okio/src/jvmTest/kotlin/okio/BufferTest.kt"), Purpose::Test);
        assert_eq!(p("src/test/java/com/x/FooTest.java"), Purpose::Test);
        assert_eq!(p("examples/basic.js"), Purpose::Example);
        assert_eq!(p("scripts/release.sh"), Purpose::Script);
        assert_eq!(p(".github/workflows/ci.yml"), Purpose::Script);
        assert_eq!(p("docs/conf.py"), Purpose::Docs);
        assert_eq!(p("benches/parse.rs"), Purpose::Benchmark);
        assert_eq!(p("testdata/certificate/key.pem"), Purpose::Test);
        assert_eq!(p("gulpfile.js"), Purpose::Script);
        assert_eq!(p("webpack.config.js"), Purpose::Script);
        assert_eq!(
            p("src/latest.py"),
            Purpose::Library,
            "a name ending in 'test' is not a test by itself"
        );
    }
}

#[cfg(test)]
mod syntax_tests {
    use super::*;

    #[test]
    fn layout_only_changes_keep_the_tree() {
        let rust = crate::lang::by_id("rust").unwrap();
        let a = "fn f() { g(a, b); }\n";
        assert!(same_syntax(
            rust,
            a,
            "fn f() {\n    g(\n        a,\n        b\n    );\n}\n"
        ));
        assert!(
            !same_syntax(rust, a, "fn f() {\n    g(\n        a,\n        b,\n    );\n}\n"),
            "a trailing comma is a token"
        );
        assert!(!same_syntax(rust, a, "fn f() { g(b, a); }\n"));
        assert!(!same_syntax(rust, a, "fn f() { g(\"a \", b); }\n"));
        assert!(!same_syntax(rust, a, "fn f() { g(a, b) }\n"));
        assert!(!same_syntax(rust, a, "fn f( { g(a, b); }\n"), "both must parse");

        // In Python, indentation is structure, not layout.
        let py = crate::lang::by_id("python").unwrap();
        let p = "if a:\n    b()\nc()\n";
        assert!(same_syntax(py, p, "if a:\n  b()\nc()\n"));
        assert!(!same_syntax(py, p, "if a:\n    b()\n    c()\n"));
        assert!(
            !same_syntax(py, "s = 'a  b'\n", "s = 'a b'\n"),
            "whitespace in a string is content"
        );
    }
}
