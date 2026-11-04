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
