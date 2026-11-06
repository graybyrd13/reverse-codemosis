//! Formatting: the sixth clause of the acceptance contract.
//!
//! Many projects keep their code exactly as a formatter prints it and check
//! that in CI. A correction that is right but leaves a line too long would
//! fail that check, and the project would be worse off. So a file that the
//! project's formatter leaves unchanged before a step must be left unchanged
//! by it after the step. Depending on `[format] mode`, a step that breaks
//! this is refused (the default), or reformatted when the formatter's
//! changes stay next to the lines the step changed.
//!
//! Detection needs an explicit signal, nearest to the file first:
//!
//! | Formatter | Files | Signal |
//! |---|---|---|
//! | rustfmt | Rust | `rustfmt.toml` / `.rustfmt.toml`, or a `Cargo.toml` |
//! | gofmt | Go | always: it is the language's standard |
//! | Black | Python | `[tool.black]` in `pyproject.toml`, or a `black` pre-commit hook |
//! | Ruff | Python | `[tool.ruff.format]`, `[format]` in `ruff.toml`, or a `ruff-format` hook |
//! | Prettier | JavaScript, TypeScript | a Prettier config file, or `prettier` in `package.json` |
//! | clang-format | C, C++ | `.clang-format` / `_clang-format` |
//!
//! A detected formatter only constrains files it already leaves unchanged,
//! so a wrong guess costs nothing: a file the formatter would rewrite is
//! simply not checked. Code is formatted through standard input, so nothing
//! is written. Prettier can execute the project's configuration and plugins,
//! so it runs only when the session may run the project's code (when the
//! build and tests are the oracle).

use codemosis_core::process;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

/// How long one formatter run may take.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// Lines of slack around a correction within which reformatting may change
/// code (a reflowed statement, a realigned block).
pub const REFORMAT_SLACK: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Rustfmt,
    Gofmt,
    Black,
    Ruff,
    Prettier,
    ClangFormat,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Rustfmt,
        Kind::Gofmt,
        Kind::Black,
        Kind::Ruff,
        Kind::Prettier,
        Kind::ClangFormat,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Rustfmt => "rustfmt",
            Kind::Gofmt => "gofmt",
            Kind::Black => "black",
            Kind::Ruff => "ruff",
            Kind::Prettier => "prettier",
            Kind::ClangFormat => "clang-format",
        }
    }

    pub fn parse(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The engine languages (by id) this formatter formats.
    pub fn languages(self) -> &'static [&'static str] {
        match self {
            Kind::Rustfmt => &["rust"],
            Kind::Gofmt => &["go"],
            Kind::Black | Kind::Ruff => &["python"],
            Kind::Prettier => &["javascript", "typescript", "tsx"],
            Kind::ClangFormat => &["c", "cpp"],
        }
    }

    /// Whether running it may execute code the project controls.
    pub fn runs_project_code(self) -> bool {
        matches!(self, Kind::Prettier)
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// What to do when a correction would leave a formatted file unformatted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Refuse the step and report why.
    #[default]
    Refuse,
    /// Apply the formatter's output, if it only changes lines next to the
    /// correction; refuse otherwise.
    Reformat,
    /// Do not check formatting.
    Off,
}

/// `[format]` in `codemosis.toml`.
#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, default)]
pub struct FormatSection {
    /// "refuse" (default): a step that would leave a formatted file
    /// unformatted is rolled back. "reformat": the formatter's output is
    /// applied when it only touches lines next to the correction. "off".
    pub mode: Mode,
    /// Formatters never to run: rustfmt, gofmt, black, ruff, prettier,
    /// clang-format.
    pub disable: Vec<String>,
}

impl FormatSection {
    /// Names in `disable` that are not formatters.
    pub fn unknown_names(&self) -> Vec<String> {
        self.disable
            .iter()
            .filter(|n| Kind::parse(n).is_none())
            .cloned()
            .collect()
    }
}

/// The formatter a file is held to, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Governing {
    pub kind: Kind,
    /// The file that says so, relative to the project root, or a reason.
    pub evidence: String,
}

/// An installed formatter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tool {
    pub program: PathBuf,
    /// The first line of its version output, e.g. `rustfmt 1.9.0-stable`.
    pub version: String,
}

/// The result of holding one changed file to its formatter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// No formatter applies, or the file was not formatted to begin with.
    Skip,
    /// The formatter leaves the corrected file as it is.
    Clean,
    /// The file was formatted before and is not after. `formatted` is what
    /// the formatter makes of it; `line` is the first line (1-based) it
    /// would change.
    Unformatted {
        kind: Kind,
        version: String,
        formatted: String,
        line: usize,
    },
    /// The formatter accepted the file before and rejects it after.
    Rejected { kind: Kind, error: String },
}

/// How one formatter was used in a session, for reports.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct Usage {
    pub formatter: String,
    pub version: Option<String>,
    /// Why the project is taken to use it (first evidence seen).
    pub evidence: String,
    /// Changed files the formatter left unchanged before the step: held to it.
    pub files_checked: usize,
    /// Changed files it would already have rewritten: not held to it.
    pub files_not_formatted: usize,
    /// Why it was not run, when it was not.
    pub not_run: Option<String>,
}

/// The formatters of one project, detected lazily and cached.
pub struct Formatters {
    root: PathBuf,
    mode: Mode,
    disabled: Vec<Kind>,
    trust_project_code: bool,
    tools: Mutex<HashMap<Kind, Result<Tool, String>>>,
    detected: Mutex<HashMap<(PathBuf, &'static str), Option<Governing>>>,
    outputs: Mutex<HashMap<(PathBuf, String), Result<String, String>>>,
    usage: Mutex<BTreeMap<Kind, Usage>>,
}

impl Formatters {
    /// `trust_project_code`: whether formatters that can execute the
    /// project's own code (Prettier) may run.
    pub fn new(root: &Path, section: &FormatSection, trust_project_code: bool) -> Formatters {
        Formatters {
            root: root.to_path_buf(),
            mode: section.mode,
            disabled: section.disable.iter().filter_map(|n| Kind::parse(n)).collect(),
            trust_project_code,
            tools: Default::default(),
            detected: Default::default(),
            outputs: Default::default(),
            usage: Default::default(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The formatter `rel` (a path relative to the root) is held to, if any.
    /// Reads configuration files only; runs nothing.
    pub fn governing(&self, rel: &Path, lang: &'static str) -> Option<Governing> {
        let dir = rel.parent().unwrap_or(Path::new("")).to_path_buf();
        let key = (dir.clone(), lang);
        if let Some(g) = self.detected.lock().expect("formatter cache").get(&key) {
            return g.clone();
        }
        let g = detect(&self.root, &dir, lang).filter(|g| !self.disabled.contains(&g.kind));
        self.detected.lock().expect("formatter cache").insert(key, g.clone());
        g
    }

    /// The installed formatter, or why it cannot be used.
    pub fn tool(&self, kind: Kind) -> Result<Tool, String> {
        if let Some(t) = self.tools.lock().expect("formatter cache").get(&kind) {
            return t.clone();
        }
        let t = if kind.runs_project_code() && !self.trust_project_code {
            Err(format!(
                "{kind} can run the project's own configuration and plugins, so it runs only when the build and tests are the oracle"
            ))
        } else {
            probe(&self.root, kind)
        };
        self.tools.lock().expect("formatter cache").insert(kind, t.clone());
        t
    }

    /// What the formatter makes of `text` as the file `rel`.
    pub fn format(&self, kind: Kind, rel: &Path, text: &str) -> Result<String, String> {
        let key = (rel.to_path_buf(), crate::proof::sha256(text));
        if let Some(out) = self.outputs.lock().expect("formatter cache").get(&key) {
            return out.clone();
        }
        let out = self.tool(kind).and_then(|tool| run(&self.root, kind, &tool, rel, text));
        self.outputs.lock().expect("formatter cache").insert(key, out.clone());
        out
    }

    /// Hold a changed file to its formatter: was it formatted before, and is
    /// it still formatted after?
    pub fn check(&self, rel: &Path, lang: &'static str, before: &str, after: &str) -> Check {
        if self.mode == Mode::Off {
            return Check::Skip;
        }
        let Some(g) = self.governing(rel, lang) else {
            return Check::Skip;
        };
        let note = |f: &dyn Fn(&mut Usage)| {
            let mut usage = self.usage.lock().expect("formatter usage");
            let u = usage.entry(g.kind).or_insert_with(|| Usage {
                formatter: g.kind.name().to_string(),
                evidence: g.evidence.clone(),
                ..Usage::default()
            });
            f(u);
        };
        let tool = match self.tool(g.kind) {
            Ok(t) => t,
            Err(why) => {
                note(&|u| u.not_run = Some(why.clone()));
                return Check::Skip;
            }
        };
        note(&|u| u.version = Some(tool.version.clone()));
        match self.format(g.kind, rel, before) {
            Ok(out) if out == before => {}
            _ => {
                note(&|u| u.files_not_formatted += 1);
                return Check::Skip;
            }
        }
        note(&|u| u.files_checked += 1);
        match self.format(g.kind, rel, after) {
            Ok(out) if out == after => Check::Clean,
            Ok(formatted) => {
                let line = first_difference(after, &formatted);
                Check::Unformatted {
                    kind: g.kind,
                    version: tool.version,
                    formatted,
                    line,
                }
            }
            Err(error) => Check::Rejected { kind: g.kind, error },
        }
    }

    /// Which formatter holds how many of these files (relative path,
    /// language id). Reads configuration only.
    pub fn survey<'a>(&self, files: impl IntoIterator<Item = (&'a Path, &'static str)>) -> Vec<(Governing, usize)> {
        let mut counts: BTreeMap<(Kind, String), usize> = BTreeMap::new();
        for (rel, lang) in files {
            if let Some(g) = self.governing(rel, lang) {
                *counts.entry((g.kind, g.evidence)).or_default() += 1;
            }
        }
        counts
            .into_iter()
            .map(|((kind, evidence), n)| (Governing { kind, evidence }, n))
            .collect()
    }

    /// How each formatter was used so far.
    pub fn usage(&self) -> Vec<Usage> {
        self.usage.lock().expect("formatter usage").values().cloned().collect()
    }
}

/// The first line (1-based) where `a` and `b` differ.
pub fn first_difference(a: &str, b: &str) -> usize {
    a.lines()
        .zip(b.lines())
        .position(|(x, y)| x != y)
        .unwrap_or_else(|| a.lines().count().min(b.lines().count()))
        + 1
}

/// Whether every line the formatter changes (`edited` → `formatted`) lies
/// within [`REFORMAT_SLACK`] lines of a line the correction changed
/// (`before` → `edited`). Returns the first line (1-based, in `edited`) it
/// would change further away.
pub fn stray_reformatting(before: &str, edited: &str, formatted: &str) -> Option<usize> {
    // Changed lines on one side of a diff; a pure insertion or deletion
    // counts as the line where it happens.
    let lines = |a: &str, b: &str, old_side: bool| -> Vec<usize> {
        similar::TextDiff::from_lines(a, b)
            .ops()
            .iter()
            .filter(|op| op.tag() != similar::DiffTag::Equal)
            .flat_map(|op| {
                let r = if old_side { op.old_range() } else { op.new_range() };
                r.start..r.end.max(r.start + 1)
            })
            .collect()
    };
    let correction = lines(before, edited, false);
    lines(edited, formatted, true)
        .into_iter()
        .find(|l| !correction.iter().any(|c| l.abs_diff(*c) <= REFORMAT_SLACK))
        .map(|l| l + 1)
}

/// Directories from `dir` (relative) up to the root, nearest first.
fn ancestors(root: &Path, dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut d = Some(dir);
    while let Some(p) = d {
        out.push(root.join(p));
        d = if p.as_os_str().is_empty() { None } else { p.parent() };
    }
    out
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn read_toml(path: &Path) -> Option<toml::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str::<toml::Table>(&text).ok().map(toml::Value::Table)
}

fn toml_has(v: &toml::Value, keys: &[&str]) -> bool {
    let mut cur = v;
    for k in keys {
        match cur.get(k) {
            Some(next) => cur = next,
            None => return false,
        }
    }
    true
}

/// A hook id in the root's `.pre-commit-config.yaml`.
fn precommit_hook(root: &Path, ids: &[&str]) -> Option<String> {
    let text = std::fs::read_to_string(root.join(".pre-commit-config.yaml")).ok()?;
    text.lines()
        .filter_map(|l| l.trim().trim_start_matches("- ").strip_prefix("id:"))
        .map(|id| id.trim().trim_matches(|c| c == '"' || c == '\''))
        .find(|id| ids.contains(id))
        .map(|id| format!(".pre-commit-config.yaml (hook {id})"))
}

fn detect(root: &Path, dir: &Path, lang: &str) -> Option<Governing> {
    let found = |kind: Kind, path: &Path| {
        Some(Governing {
            kind,
            evidence: relative(root, path),
        })
    };
    match lang {
        "go" => Some(Governing {
            kind: Kind::Gofmt,
            evidence: "gofmt is the Go standard".to_string(),
        }),
        "rust" => {
            let dirs = ancestors(root, dir);
            for d in &dirs {
                for name in ["rustfmt.toml", ".rustfmt.toml"] {
                    if d.join(name).is_file() {
                        return found(Kind::Rustfmt, &d.join(name));
                    }
                }
            }
            dirs.iter()
                .map(|d| d.join("Cargo.toml"))
                .find(|p| p.is_file())
                .and_then(|p| found(Kind::Rustfmt, &p))
        }
        "python" => {
            for d in ancestors(root, dir) {
                let py = d.join("pyproject.toml");
                if let Some(v) = py.is_file().then(|| read_toml(&py)).flatten() {
                    if toml_has(&v, &["tool", "black"]) {
                        return found(Kind::Black, &py);
                    }
                    if toml_has(&v, &["tool", "ruff", "format"]) {
                        return found(Kind::Ruff, &py);
                    }
                }
                for name in ["ruff.toml", ".ruff.toml"] {
                    let p = d.join(name);
                    if read_toml(&p).is_some_and(|v| toml_has(&v, &["format"])) {
                        return found(Kind::Ruff, &p);
                    }
                }
            }
            if let Some(e) = precommit_hook(root, &["black"]) {
                return Some(Governing {
                    kind: Kind::Black,
                    evidence: e,
                });
            }
            precommit_hook(root, &["ruff-format"]).map(|e| Governing {
                kind: Kind::Ruff,
                evidence: e,
            })
        }
        "javascript" | "typescript" | "tsx" => {
            const CONFIGS: &[&str] = &[
                ".prettierrc",
                ".prettierrc.json",
                ".prettierrc.json5",
                ".prettierrc.yaml",
                ".prettierrc.yml",
                ".prettierrc.toml",
                ".prettierrc.js",
                ".prettierrc.cjs",
                ".prettierrc.mjs",
                ".prettierrc.ts",
                "prettier.config.js",
                "prettier.config.cjs",
                "prettier.config.mjs",
                "prettier.config.ts",
            ];
            for d in ancestors(root, dir) {
                for name in CONFIGS {
                    if d.join(name).is_file() {
                        return found(Kind::Prettier, &d.join(name));
                    }
                }
                let pkg = d.join("package.json");
                let uses = std::fs::read_to_string(&pkg)
                    .ok()
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .is_some_and(|v| {
                        v.get("prettier").is_some()
                            || ["devDependencies", "dependencies"]
                                .iter()
                                .any(|k| v.get(k).and_then(|d| d.get("prettier")).is_some())
                    });
                if uses {
                    return found(Kind::Prettier, &pkg);
                }
            }
            None
        }
        "c" | "cpp" => ancestors(root, dir)
            .into_iter()
            .flat_map(|d| [d.join(".clang-format"), d.join("_clang-format")])
            .find(|p| p.is_file())
            .and_then(|p| found(Kind::ClangFormat, &p)),
        _ => None,
    }
}

fn program(root: &Path, kind: Kind) -> PathBuf {
    if kind == Kind::Prettier {
        let local = root.join("node_modules/.bin/prettier");
        if local.is_file() {
            return local;
        }
    }
    PathBuf::from(kind.name())
}

fn probe(root: &Path, kind: Kind) -> Result<Tool, String> {
    let program = program(root, kind);
    let not_installed = |e: String| format!("{kind} is not installed or cannot run ({e})");
    let version = if kind == Kind::Gofmt {
        // gofmt has no version flag: check it runs, and report Go's version.
        let r = process::run_with_input(&mut Command::new(&program), "package p\n", TIMEOUT)
            .map_err(|e| not_installed(e.to_string()))?;
        if !r.success() {
            return Err(not_installed(r.stderr.trim().to_string()));
        }
        // GOTOOLCHAIN=local: inside a module, `go` may otherwise switch to
        // (and download) the toolchain the module asks for.
        process::run(Command::new("go").arg("version").env("GOTOOLCHAIN", "local"), TIMEOUT)
            .ok()
            .filter(|r| r.success())
            .and_then(|r| r.stdout.split_whitespace().nth(2).map(|v| format!("gofmt ({v})")))
            .unwrap_or_else(|| "gofmt".to_string())
    } else {
        let r =
            process::run(Command::new(&program).arg("--version"), TIMEOUT).map_err(|e| not_installed(e.to_string()))?;
        if !r.success() {
            return Err(not_installed(r.combined().trim().to_string()));
        }
        let first = r.stdout.lines().next().unwrap_or("").trim().to_string();
        if kind == Kind::Prettier {
            format!("prettier {first}")
        } else {
            first
        }
    };
    Ok(Tool { program, version })
}

/// The edition rustfmt should parse with: `rustfmt.toml`'s own setting
/// wins, then the nearest `Cargo.toml` (following `edition.workspace`),
/// then 2015, Cargo's default.
fn rust_edition(root: &Path, dir: &Path) -> Option<String> {
    let dirs = ancestors(root, dir);
    for d in &dirs {
        for name in ["rustfmt.toml", ".rustfmt.toml"] {
            if let Some(v) = read_toml(&d.join(name)) {
                if v.get("edition").is_some() {
                    return None;
                }
            }
        }
    }
    let mut inherit = false;
    for d in &dirs {
        let Some(v) = read_toml(&d.join("Cargo.toml")) else {
            continue;
        };
        if !inherit {
            match v.get("package").and_then(|p| p.get("edition")) {
                Some(toml::Value::String(e)) => return Some(e.clone()),
                Some(toml::Value::Table(_)) => inherit = true,
                _ => {}
            }
        }
        if let Some(e) = v
            .get("workspace")
            .and_then(|w| w.get("package"))
            .and_then(|p| p.get("edition"))
            .and_then(|e| e.as_str())
        {
            if inherit || v.get("package").is_none() {
                return Some(e.to_string());
            }
        }
        if !inherit && v.get("package").is_some() {
            break;
        }
    }
    Some("2015".to_string())
}

fn run(root: &Path, kind: Kind, tool: &Tool, rel: &Path, text: &str) -> Result<String, String> {
    let abs = root.join(rel);
    let mut dir = abs.parent().unwrap_or(root).to_path_buf();
    if !dir.is_dir() {
        // A file being created, or a project held in memory.
        dir = if root.is_dir() {
            root.to_path_buf()
        } else {
            PathBuf::from(".")
        };
    }
    let root = if root.is_dir() { root } else { Path::new(".") };
    let mut cmd = Command::new(&tool.program);
    match kind {
        Kind::Rustfmt => {
            cmd.args(["--emit", "stdout"]);
            if let Some(edition) = rust_edition(root, rel.parent().unwrap_or(Path::new(""))) {
                cmd.args(["--edition", &edition]);
            }
            // rustfmt finds its configuration from the working directory.
            cmd.current_dir(&dir);
        }
        Kind::Gofmt => {
            cmd.current_dir(&dir);
        }
        Kind::Black => {
            cmd.args(["-q", "--stdin-filename"])
                .arg(&abs)
                .arg("-")
                .current_dir(root);
        }
        Kind::Ruff => {
            cmd.args(["format", "-q", "--stdin-filename"])
                .arg(&abs)
                .arg("-")
                .current_dir(root);
        }
        Kind::Prettier => {
            cmd.arg("--stdin-filepath").arg(&abs).current_dir(root);
        }
        Kind::ClangFormat => {
            cmd.arg(format!("--assume-filename={}", abs.display()))
                .current_dir(&dir);
        }
    }
    cmd.env("NO_COLOR", "1");
    let r = process::run_with_input(&mut cmd, text, TIMEOUT).map_err(|e| format!("{kind} could not run: {e}"))?;
    if r.timed_out {
        return Err(format!("{kind} timed out after {}s", TIMEOUT.as_secs()));
    }
    if !r.success() {
        let msg = r.combined();
        let msg = msg.trim();
        return Err(if msg.is_empty() {
            format!("{kind} failed with status {:?}", r.code)
        } else {
            msg.lines().take(5).collect::<Vec<_>>().join("\n")
        });
    }
    Ok(r.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(files: &[(&str, &str)]) -> codemosis_core::process::ScratchDir {
        let dir = codemosis_core::process::ScratchDir::new("formatter").unwrap();
        for (path, text) in files {
            let p = dir.path.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        dir
    }

    #[test]
    fn detection_needs_a_signal_and_prefers_the_nearest() {
        let d = scratch(&[
            ("Cargo.toml", "[package]\nname = \"x\"\n"),
            ("sub/rustfmt.toml", "max_width = 79\n"),
            ("py/pyproject.toml", "[tool.black]\nline-length = 88\n"),
            ("rf/ruff.toml", "[format]\nquote-style = \"single\"\n"),
            ("lint-only/pyproject.toml", "[tool.ruff]\nselect = [\"E\"]\n"),
            ("web/package.json", "{\"devDependencies\": {\"prettier\": \"3\"}}"),
            ("c/.clang-format", "BasedOnStyle: LLVM\n"),
        ]);
        let f = Formatters::new(&d.path, &FormatSection::default(), true);
        let g = |p: &str, l| f.governing(Path::new(p), l).map(|g| (g.kind, g.evidence));
        assert_eq!(g("src/a.rs", "rust"), Some((Kind::Rustfmt, "Cargo.toml".into())));
        assert_eq!(
            g("sub/deep/a.rs", "rust"),
            Some((Kind::Rustfmt, "sub/rustfmt.toml".into()))
        );
        assert_eq!(g("main.go", "go").map(|g| g.0), Some(Kind::Gofmt));
        assert_eq!(
            g("py/pkg/a.py", "python"),
            Some((Kind::Black, "py/pyproject.toml".into()))
        );
        assert_eq!(g("rf/a.py", "python"), Some((Kind::Ruff, "rf/ruff.toml".into())));
        assert_eq!(
            g("lint-only/a.py", "python"),
            None,
            "a lint configuration is not a formatter"
        );
        assert_eq!(g("other/a.py", "python"), None);
        assert_eq!(g("web/src/a.ts", "typescript").map(|g| g.0), Some(Kind::Prettier));
        assert_eq!(g("src/a.js", "javascript"), None);
        assert_eq!(g("c/x/a.c", "c").map(|g| g.0), Some(Kind::ClangFormat));
        assert_eq!(g("a.c", "c"), None);

        let off = Formatters::new(
            &d.path,
            &FormatSection {
                disable: vec!["gofmt".into()],
                ..FormatSection::default()
            },
            true,
        );
        assert_eq!(off.governing(Path::new("main.go"), "go"), None);
    }

    #[test]
    fn precommit_hooks_count() {
        let d = scratch(&[(
            ".pre-commit-config.yaml",
            "repos:\n  - repo: https://github.com/astral-sh/ruff-pre-commit\n    hooks:\n      - id: ruff\n      - id: ruff-format\n",
        )]);
        let f = Formatters::new(&d.path, &FormatSection::default(), true);
        let g = f.governing(Path::new("a.py"), "python").unwrap();
        assert_eq!(g.kind, Kind::Ruff);
        assert!(g.evidence.contains("ruff-format"), "{}", g.evidence);
    }

    #[test]
    fn editions_follow_cargo() {
        let d = scratch(&[
            ("Cargo.toml", "[workspace]\n[workspace.package]\nedition = \"2021\"\n"),
            (
                "crates/a/Cargo.toml",
                "[package]\nname = \"a\"\nedition.workspace = true\n",
            ),
            ("crates/b/Cargo.toml", "[package]\nname = \"b\"\nedition = \"2024\"\n"),
            ("crates/c/Cargo.toml", "[package]\nname = \"c\"\n"),
            ("own/rustfmt.toml", "edition = \"2018\"\n"),
        ]);
        let e = |p: &str| rust_edition(&d.path, Path::new(p));
        assert_eq!(e("crates/a/src").as_deref(), Some("2021"));
        assert_eq!(e("crates/b/src").as_deref(), Some("2024"));
        assert_eq!(e("crates/c/src").as_deref(), Some("2015"));
        assert_eq!(e("own"), None, "rustfmt.toml's own edition wins");
    }

    #[test]
    fn stray_reformatting_is_found() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n";
        let edited = "a\nb\nC\nd\ne\nf\ng\nh\ni\nj\n";
        // Reformatting next to the correction is fine.
        assert_eq!(
            stray_reformatting(before, edited, "a\nb\nC1\nC2\nd\ne\nf\ng\nh\ni\nj\n"),
            None
        );
        assert_eq!(
            stray_reformatting(before, edited, "a\nb\nC\nd\nE\nf\ng\nh\ni\nj\n"),
            None
        );
        // Far from it is not.
        assert_eq!(
            stray_reformatting(before, edited, "a\nb\nC\nd\ne\nf\ng\nh\ni\nJ\n"),
            Some(10)
        );
    }

    #[test]
    fn first_difference_counts_lines() {
        assert_eq!(first_difference("a\nb\nc\n", "a\nB\nc\n"), 2);
        assert_eq!(first_difference("a\n", "a\nb\n"), 2);
    }

    /// Run only where the formatter is installed; CI installs rustfmt and gofmt.
    fn installed(kind: Kind) -> bool {
        probe(Path::new("."), kind).is_ok()
    }

    #[test]
    fn rustfmt_holds_a_formatted_file_to_its_width() {
        if !installed(Kind::Rustfmt) {
            eprintln!("rustfmt is not installed; skipped");
            return;
        }
        let d = scratch(&[
            ("Cargo.toml", "[package]\nname = \"x\"\nedition = \"2021\"\n"),
            ("rustfmt.toml", "max_width = 40\n"),
        ]);
        let f = Formatters::new(&d.path, &FormatSection::default(), false);
        let rel = Path::new("src/lib.rs");
        let before = "fn f(a: bool) -> bool {\n    if a {\n        return true;\n    }\n    false\n}\n";
        let after = "fn f(a: bool) -> bool {\n    if a { return true; }\n    false\n}\n";
        match f.check(rel, "rust", before, after) {
            Check::Unformatted {
                kind, line, formatted, ..
            } => {
                assert_eq!(kind, Kind::Rustfmt);
                assert_eq!(line, 2);
                assert_eq!(formatted, before);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(f.check(rel, "rust", before, before), Check::Clean);
        // A file rustfmt would rewrite anyway is not held to it.
        assert_eq!(f.check(rel, "rust", after, after), Check::Skip);
        let usage = f.usage();
        assert_eq!(usage.len(), 1);
        assert_eq!(usage[0].files_checked, 2);
        assert_eq!(usage[0].files_not_formatted, 1);
    }

    #[test]
    fn gofmt_rejects_what_it_cannot_parse() {
        if !installed(Kind::Gofmt) {
            eprintln!("gofmt is not installed; skipped");
            return;
        }
        let d = scratch(&[]);
        let f = Formatters::new(&d.path, &FormatSection::default(), false);
        let rel = Path::new("main.go");
        let before = "package main\n\nfunc main() {\n\tx := 1\n\t_ = x\n}\n";
        assert_eq!(f.check(rel, "go", before, before), Check::Clean);
        let broken = "package main\n\nfunc main( {\n}\n";
        assert!(matches!(f.check(rel, "go", before, broken), Check::Rejected { .. }));
    }

    #[test]
    fn prettier_waits_for_trust() {
        let d = scratch(&[(".prettierrc", "{}")]);
        let f = Formatters::new(&d.path, &FormatSection::default(), false);
        assert!(f
            .tool(Kind::Prettier)
            .unwrap_err()
            .contains("only when the build and tests"));
        assert_eq!(f.check(Path::new("a.js"), "javascript", "x;\n", "y;\n"), Check::Skip);
        assert!(f.usage()[0].not_run.is_some());
    }

    #[test]
    fn off_checks_nothing() {
        let d = scratch(&[]);
        let f = Formatters::new(
            &d.path,
            &FormatSection {
                mode: Mode::Off,
                ..FormatSection::default()
            },
            true,
        );
        assert_eq!(
            f.check(Path::new("main.go"), "go", "package main\n", "package  main\n"),
            Check::Skip
        );
    }

    /// Each formatter, end to end: a formatted file is held to it, a stray
    /// space is caught at its line. Skipped where the formatter is missing.
    #[test]
    fn every_formatter_holds_its_files() {
        /// formatter, config files, file, language, formatted, one stray space
        type Case<'a> = (Kind, &'a [(&'a str, &'a str)], &'a str, &'static str, &'a str, &'a str);
        let cases: &[Case] = &[
            (
                Kind::Black,
                &[("pyproject.toml", "[tool.black]\n")],
                "pkg/a.py",
                "python",
                "x = 1\ny = 2\n",
                "x = 1\ny  = 2\n",
            ),
            (
                Kind::Ruff,
                &[("ruff.toml", "[format]\n")],
                "pkg/a.py",
                "python",
                "x = 1\ny = 2\n",
                "x = 1\ny  = 2\n",
            ),
            (
                Kind::Prettier,
                &[(".prettierrc", "{}\n")],
                "src/a.js",
                "javascript",
                "const x = 1;\nconst y = 2;\n",
                "const x = 1;\nconst y  = 2;\n",
            ),
            (
                Kind::ClangFormat,
                &[(".clang-format", "BasedOnStyle: LLVM\n")],
                "src/a.c",
                "c",
                "int x = 1;\nint y = 2;\n",
                "int x = 1;\nint y  = 2;\n",
            ),
        ];
        for (kind, files, rel, lang, clean, sloppy) in cases {
            if !installed(*kind) {
                eprintln!("{kind} is not installed; skipped");
                continue;
            }
            let d = scratch(files);
            let f = Formatters::new(&d.path, &FormatSection::default(), true);
            let rel = Path::new(rel);
            assert_eq!(f.governing(rel, lang).map(|g| g.kind), Some(*kind));
            assert_eq!(f.check(rel, lang, clean, clean), Check::Clean, "{kind}");
            match f.check(rel, lang, clean, sloppy) {
                Check::Unformatted { line, formatted, .. } => {
                    assert_eq!(line, 2, "{kind}");
                    assert_eq!(&formatted, clean, "{kind}");
                }
                other => panic!("{kind}: {other:?}"),
            }
        }
    }
}
