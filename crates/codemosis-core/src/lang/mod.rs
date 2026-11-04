//! Languages the engine can read.
//!
//! A language supplies a tree-sitter grammar and, optionally, its own facts,
//! template filters, import placement and a runner that executes snippets so
//! pack examples can be checked by running them.

use crate::eval::{EvalCtx, EvalError, FactDoc, Value};
use crate::model::Edit;
use crate::project::SourceFile;
use tree_sitter::Node;

pub mod generic;
pub mod python;
mod python_scope;
pub mod rust;

/// The outcome of running a snippet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunStatus {
    /// Compiled (and, for `run`, exited with status 0).
    Ok,
    /// Did not compile.
    CompileError,
    /// Compiled, then exited with a non-zero status (or a signal).
    Failed(Option<i32>),
    TimedOut,
    /// The toolchain is not installed.
    Unavailable(String),
}

#[derive(Clone, Debug)]
pub struct RunOutcome {
    pub status: RunStatus,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutcome {
    pub fn unavailable(why: impl Into<String>) -> RunOutcome {
        RunOutcome {
            status: RunStatus::Unavailable(why.into()),
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    /// The last non-empty line of stderr: for most runtimes, the error.
    pub fn error_line(&self) -> String {
        self.stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .to_string()
    }
}

/// Compiles and runs snippets of a language, for pack self-tests.
pub trait Runner: Send + Sync {
    fn describe(&self) -> String;
    /// Check that the code compiles, without running it.
    fn compile(&self, code: &str) -> RunOutcome;
    /// Compile and run the code.
    fn run(&self, code: &str) -> RunOutcome;
}

pub trait Language: Send + Sync + 'static {
    /// Stable id, used in pack manifests: `python`.
    fn id(&self) -> &'static str;
    /// Prefix of this language's facts and filters: `py`.
    fn prefix(&self) -> &'static str;
    /// The Linguist languages this grammar parses, e.g. `["Python"]`.
    fn linguist_names(&self) -> &'static [&'static str];
    fn grammar(&self) -> tree_sitter::Language;
    /// File name used for single-file pack examples.
    fn example_file(&self) -> &'static str;

    fn is_comment(&self, node: &Node) -> bool {
        node.kind().contains("comment")
    }

    /// True for string and character literals (whose contents are not code).
    fn is_string(&self, node: &Node) -> bool {
        STRING_KINDS.contains(&node.kind())
    }

    /// Node kinds that define something, with the field holding the name.
    fn definition_kinds(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// The name of a definition node (function, class...), used to anchor
    /// findings to code rather than to line numbers.
    fn definition_name(&self, _node: &Node, _text: &str) -> Option<String> {
        None
    }

    fn facts(&self) -> &'static [FactDoc] {
        &[]
    }

    /// Evaluate a language fact (`name` without the prefix). `None` means the
    /// fact does not exist.
    fn call<'a>(&self, _name: &str, _args: &[Value<'a>], _ctx: &EvalCtx<'a>) -> Option<Result<Value<'a>, EvalError>> {
        None
    }

    fn filters(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    fn filter<'a>(
        &self,
        _name: &str,
        _arg: Option<&str>,
        _value: &Value<'a>,
        _ctx: &EvalCtx<'a>,
    ) -> Option<Result<Value<'a>, EvalError>> {
        None
    }

    /// The edit that adds an import statement in the conventional place.
    fn add_import(&self, _file: &SourceFile, _statement: &str) -> Result<Edit, String> {
        Err(format!("{} does not support add_import", self.id()))
    }

    /// One level of indentation as this file writes it.
    fn indent_unit(&self, text: &str) -> String {
        detect_indent(text)
    }

    fn runner(&self) -> Option<&dyn Runner> {
        None
    }

    /// References outside `edits` whose binding the edits change: each
    /// message names the reference and the change. `repaired` allows a name
    /// that resolved to nothing (or a builtin) to become bound, which is
    /// what repairing a missing import does. `None` means the language has
    /// no binding analysis, so nothing is checked.
    fn binding_changes(
        &self,
        _old: &SourceFile,
        _new: &SourceFile,
        _edits: &[Edit],
        _repaired: bool,
    ) -> Option<Vec<String>> {
        None
    }
}

/// Where an old offset lands after `edits` (sorted, non-overlapping), or
/// None when the offset is strictly inside replaced text. Text inserted at
/// the offset itself goes before it, so it shifts the offset.
pub fn map_offset(edits: &[Edit], pos: usize) -> Option<usize> {
    let mut shift: isize = 0;
    for e in edits {
        if e.end <= pos {
            shift += e.text.len() as isize - (e.end - e.start) as isize;
        } else if e.start < pos && pos < e.end {
            return None;
        }
    }
    Some((pos as isize + shift) as usize)
}

pub fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let ws: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        if ws.starts_with('\t') {
            return "\t".to_string();
        }
        if !ws.is_empty() {
            return ws;
        }
    }
    "    ".to_string()
}

/// Node kinds that are string or character literals in the grammars here.
pub const STRING_KINDS: &[&str] = &[
    "string",
    "string_literal",
    "raw_string_literal",
    "char_literal",
    "character_literal",
    "template_string",
    "interpreted_string_literal",
    "rune_literal",
    "text_block",
    "verbatim_string_literal",
    "interpolated_string_expression",
    "encapsed_string",
    "heredoc",
    "nowdoc",
    "heredoc_body",
    "raw_string",
    "ansi_c_string",
    "line_string_literal",
    "multi_line_string_literal",
    "regex",
    "system_lib_string",
    "concatenated_string",
];

static PYTHON: python::Python = python::Python;
static RUST: rust::Rust = rust::Rust;

/// Every language with a grammar in this build.
pub fn all() -> Vec<&'static dyn Language> {
    let mut v: Vec<&'static dyn Language> = vec![&PYTHON, &RUST];
    v.extend(generic::LANGUAGES.iter().map(|l| l as &'static dyn Language));
    v
}

pub fn by_id(id: &str) -> Option<&'static dyn Language> {
    all().into_iter().find(|l| l.id() == id)
}

/// The grammar for a Linguist language, if this build has one.
pub fn for_linguist(name: &str) -> Option<&'static dyn Language> {
    all().into_iter().find(|l| l.linguist_names().contains(&name))
}

include!(concat!(env!("OUT_DIR"), "/grammar_versions.rs"));

/// The crate that provides a grammar.
pub fn grammar_crate(id: &str) -> &'static str {
    match id {
        "python" => "tree-sitter-python",
        "rust" => "tree-sitter-rust",
        "javascript" => "tree-sitter-javascript",
        "typescript" | "tsx" => "tree-sitter-typescript",
        "go" => "tree-sitter-go",
        "java" => "tree-sitter-java",
        "c" => "tree-sitter-c",
        "cpp" => "tree-sitter-cpp",
        "csharp" => "tree-sitter-c-sharp",
        "ruby" => "tree-sitter-ruby",
        "php" => "tree-sitter-php",
        "bash" => "tree-sitter-bash",
        "kotlin" => "tree-sitter-kotlin-ng",
        "scala" => "tree-sitter-scala",
        "lua" => "tree-sitter-lua",
        "swift" => "tree-sitter-swift",
        _ => "",
    }
}

/// The exact version of a grammar's crate in this build, when known.
pub fn grammar_version(id: &str) -> Option<&'static str> {
    let krate = grammar_crate(id);
    GRAMMAR_VERSIONS.iter().find(|(n, _)| *n == krate).map(|(_, v)| *v)
}

/// The tree-sitter runtime version in this build, when known.
pub fn runtime_version() -> Option<&'static str> {
    GRAMMAR_VERSIONS
        .iter()
        .find(|(n, _)| *n == "tree-sitter")
        .map(|(_, v)| *v)
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn every_grammar_has_a_pinned_version() {
        if GRAMMAR_VERSIONS.is_empty() {
            return; // built outside the workspace, without its Cargo.lock
        }
        for l in all() {
            assert!(grammar_version(l.id()).is_some(), "no version for {}", l.id());
        }
        assert!(runtime_version().is_some());
    }
}

#[cfg(test)]
mod map_tests {
    use super::*;

    #[test]
    fn offsets_move_with_the_edits_before_them() {
        let e = |start, end, text: &str| Edit {
            start,
            end,
            text: text.to_string(),
        };
        let edits = [e(0, 0, "import json\n"), e(10, 14, "x"), e(20, 20, "!!")];
        assert_eq!(map_offset(&edits, 0), Some(12));
        assert_eq!(map_offset(&edits, 5), Some(17));
        assert_eq!(map_offset(&edits, 12), None);
        assert_eq!(map_offset(&edits, 14), Some(23));
        assert_eq!(map_offset(&edits, 20), Some(31));
    }
}
