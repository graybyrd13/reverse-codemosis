//! Fix packs.
//!
//! A pack is a directory:
//!
//! ```text
//! my-pack/
//!   pack.toml          # [pack] id, version, language, summary, license, authors
//!   rules/*.toml       # one rule per file
//!   tables/*.toml      # optional data tables the rules look up
//! ```
//!
//! See `docs/PACK_FORMAT.md` for the full format.

use crate::lang::{self, Language};
use crate::model::{Category, Semantics};
use crate::rule::CompiledRule;
use crate::tables::Tables;
use serde::Deserialize;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackMeta {
    pub id: String,
    pub version: String,
    /// A grammar id (`python`), or `*` for rules that read any text file.
    #[serde(default)]
    pub language: Option<String>,
    /// Several grammar ids whose node kinds the rules share
    /// (`["javascript", "typescript", "tsx"]`).
    #[serde(default)]
    pub languages: Vec<String>,
    pub summary: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
    /// The engine versions this pack works with, like ">=0.1, <0.3".
    /// A pack whose requirement the running engine does not meet does not
    /// load, rather than silently misbehaving.
    #[serde(default)]
    pub engine: Option<String>,
    /// Set when the pack should no longer be used: why, and what instead.
    #[serde(default)]
    pub deprecated: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    pack: PackMeta,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSpec {
    pub id: String,
    pub title: String,
    pub category: Category,
    #[serde(default)]
    pub semantics: Option<Semantics>,
    /// One sentence, rendered with captures, shown for each finding.
    pub summary: String,
    /// The plain-language explanation shown by `codemosis explain`.
    pub explain: String,
    /// A tree-sitter query. Captures name the parts the rule talks about.
    #[serde(default)]
    pub shape: Option<String>,
    /// A regular expression matched against the text, for rules that work
    /// on any language. Named groups become captures; `@match` is the whole match.
    #[serde(default)]
    pub pattern: Option<String>,
    /// The name of a table mapping GitHub language names to regular
    /// expressions: the rule uses the patterns of each file's language.
    #[serde(default)]
    pub patterns: Option<String>,
    /// Where a text match must start: "code" (default), "comment", "string" or "any".
    #[serde(default)]
    pub within: Option<String>,
    /// Facts that must all hold.
    #[serde(default, rename = "where")]
    pub facts: Vec<String>,
    /// Derived captures, evaluated in order: `name = "expression"`.
    #[serde(default, rename = "let")]
    pub lets: toml::Table,
    /// The capture reported as the finding's location.
    #[serde(default)]
    pub focus: Option<String>,
    /// Templates that identify a finding; matches with equal anchors merge.
    #[serde(default)]
    pub anchor: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<EvidenceSpec>,
    #[serde(default)]
    pub fix: Option<FixSpec>,
    #[serde(default)]
    pub ask: Option<AskSpec>,
    #[serde(default)]
    pub references: Vec<String>,
    /// Free-form labels for filtering and reports ("performance", "async").
    #[serde(default)]
    pub tags: Vec<String>,
    /// Common Weakness Enumeration ids the rule addresses ("CWE-396").
    #[serde(default)]
    pub cwe: Vec<String>,
    /// Set when the rule should no longer be used: why, and what instead.
    /// A deprecated rule still runs; its findings and `explain` say so.
    #[serde(default)]
    pub deprecated: Option<String>,
    /// The rule that replaces this one, as a qualified id.
    #[serde(default)]
    pub replaced_by: Option<String>,
    #[serde(default, rename = "example")]
    pub examples: Vec<ExampleSpec>,
    #[serde(default, rename = "counterexample")]
    pub counterexamples: Vec<CounterexampleSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSpec {
    pub at: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixSpec {
    pub edits: Vec<EditSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AskSpec {
    /// Without options, phrased so that "yes" applies the `fix`.
    pub question: String,
    /// Named answers, each with its own edits (none means keep the code).
    #[serde(default, rename = "option")]
    pub options: Vec<OptionSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionSpec {
    pub answer: String,
    /// What choosing this answer means, in a few words.
    pub means: String,
    #[serde(default)]
    pub edits: Vec<EditSpec>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditSpec {
    pub replace: Option<String>,
    pub delete: Option<String>,
    pub delete_lines: Option<String>,
    pub delete_item: Option<String>,
    pub insert_before: Option<String>,
    pub insert_after: Option<String>,
    pub insert_lines_before: Option<String>,
    pub insert_lines_after: Option<String>,
    pub add_import: Option<String>,
    pub with: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExampleSpec {
    #[serde(default)]
    pub name: Option<String>,
    /// File path for the example (defaults to the language's example file).
    #[serde(default)]
    pub path: Option<String>,
    pub before: String,
    /// Expected result of applying the rule until it no longer matches.
    /// Required for rules with a fix, forbidden for explain-only rules.
    #[serde(default)]
    pub after: Option<String>,
    /// Expected standard output when `after` runs.
    #[serde(default)]
    pub output: Option<String>,
    /// Expected standard output when `before` runs.
    #[serde(default)]
    pub output_before: Option<String>,
    /// Set to false when the example cannot be executed on its own.
    #[serde(default)]
    pub run: Option<bool>,
    /// The answer to the rule's question in this example (default: "yes",
    /// or the first option that changes code).
    #[serde(default)]
    pub answer: Option<String>,
    /// For packs with several grammars: which one this example is in.
    #[serde(default)]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    pub code: String,
    #[serde(default)]
    pub language: Option<String>,
}

/// The raw files of a pack, before compilation.
#[derive(Debug, Clone)]
pub struct PackSource {
    /// Where the pack came from: a directory, or `builtin:<dir>`.
    pub origin: String,
    pub manifest: String,
    /// (file name, contents), sorted by file name.
    pub rules: Vec<(String, String)>,
    pub tables: Vec<(String, String)>,
}

/// Every problem found while compiling a pack.
#[derive(Debug, Clone)]
pub struct PackError {
    pub origin: String,
    pub problems: Vec<String>,
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "pack {} has {} problem(s):", self.origin, self.problems.len())?;
        for p in &self.problems {
            writeln!(f, "  - {p}")?;
        }
        Ok(())
    }
}

impl std::error::Error for PackError {}

/// A compiled, validated pack.
pub struct Pack {
    pub meta: PackMeta,
    pub origin: String,
    /// The grammars its rules are written for; empty for text packs, which
    /// read any file.
    pub languages: Vec<&'static dyn Language>,
    pub rules: Vec<CompiledRule>,
    pub tables: Arc<Tables>,
}

impl Pack {
    pub fn is_text(&self) -> bool {
        self.languages.is_empty()
    }

    /// "python", "javascript+typescript+tsx", or "any text".
    pub fn target(&self) -> String {
        if self.is_text() {
            "any text".to_string()
        } else {
            self.languages.iter().map(|l| l.id()).collect::<Vec<_>>().join("+")
        }
    }

    /// True when the pack applies to a project containing these grammars.
    pub fn applies_to(&self, grammar_ids: &[&str]) -> bool {
        self.is_text() || self.languages.iter().any(|l| grammar_ids.contains(&l.id()))
    }
}

impl fmt::Debug for Pack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pack")
            .field("id", &self.meta.id)
            .field("rules", &self.rules.len())
            .finish()
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Read a pack directory.
pub fn read_dir(dir: &Path) -> Result<PackSource, PackError> {
    let origin = dir.display().to_string();
    let fail = |m: String| PackError {
        origin: origin.clone(),
        problems: vec![m],
    };
    let manifest =
        std::fs::read_to_string(dir.join("pack.toml")).map_err(|e| fail(format!("cannot read pack.toml: {e}")))?;
    let read_toml_files = |sub: &str| -> Result<Vec<(String, String)>, PackError> {
        let d = dir.join(sub);
        if !d.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        let entries = std::fs::read_dir(&d).map_err(|e| fail(format!("cannot read {sub}/: {e}")))?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("toml") {
                let name = format!("{sub}/{}", p.file_name().unwrap_or_default().to_string_lossy());
                let text = std::fs::read_to_string(&p).map_err(|e| fail(format!("cannot read {name}: {e}")))?;
                out.push((name, text));
            }
        }
        out.sort();
        Ok(out)
    };
    Ok(PackSource {
        origin: origin.clone(),
        manifest,
        rules: read_toml_files("rules")?,
        tables: read_toml_files("tables")?,
    })
}

/// Validate and compile a pack.
pub fn compile(src: &PackSource) -> Result<Pack, PackError> {
    let mut problems = Vec::new();
    let manifest: ManifestFile = match toml::from_str(&src.manifest) {
        Ok(m) => m,
        Err(e) => {
            return Err(PackError {
                origin: src.origin.clone(),
                problems: vec![format!("pack.toml: {e}")],
            })
        }
    };
    let meta = manifest.pack;
    if !valid_id(&meta.id) {
        problems.push(format!(
            "pack.toml: id '{}' must be lowercase letters, digits and dashes",
            meta.id
        ));
    }
    if let Some(req) = &meta.engine {
        match crate::version::Requirement::parse(req) {
            Ok(r) if !r.matches(crate::version::engine()) => {
                return Err(PackError {
                    origin: src.origin.clone(),
                    problems: vec![format!(
                        "pack.toml: this pack needs engine {r}, and this is {}; use a matching version of codemosis or of the pack",
                        crate::version::ENGINE_VERSION
                    )],
                })
            }
            Ok(_) => {}
            Err(e) => problems.push(format!("pack.toml: engine: {e}")),
        }
    }
    if crate::version::Version::parse(&meta.version).is_err() {
        problems.push(format!("pack.toml: version '{}' is not like 1.2.3", meta.version));
    }
    let ids: Vec<String> = match (&meta.language, meta.languages.is_empty()) {
        (Some(l), true) if l == "*" => Vec::new(),
        (Some(l), true) => vec![l.clone()],
        (None, false) => meta.languages.clone(),
        _ => {
            return Err(PackError {
                origin: src.origin.clone(),
                problems: vec![
                    "pack.toml: give either `language` (a grammar id, or \"*\" for any text) or `languages` (a list of grammar ids)"
                        .to_string(),
                ],
            })
        }
    };
    let mut languages = Vec::new();
    for id in &ids {
        match lang::by_id(id) {
            Some(l) => languages.push(l),
            None => {
                let known: Vec<&str> = lang::all().iter().map(|l| l.id()).collect();
                return Err(PackError {
                    origin: src.origin.clone(),
                    problems: vec![format!(
                        "pack.toml: unknown language '{id}' (grammars in this build: {}; use \"*\" for rules that read any text)",
                        known.join(", ")
                    )],
                });
            }
        }
    }
    let mut tables = Tables::default();
    for (name, text) in &src.tables {
        if let Err(e) = tables.merge_toml(text) {
            problems.push(format!("{name}: {e}"));
        }
    }
    let tables = Arc::new(tables);
    let mut rules: Vec<CompiledRule> = Vec::new();
    if src.rules.is_empty() {
        problems.push("the pack has no rules (rules/*.toml)".to_string());
    }
    for (name, text) in &src.rules {
        let spec: RuleSpec = match toml::from_str(text) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        let expected_file = format!("rules/{}.toml", spec.id);
        if *name != expected_file && src.origin.starts_with("builtin:") {
            problems.push(format!(
                "{name}: a rule's file must be named after its id ({expected_file})"
            ));
        }
        if rules.iter().any(|r| r.spec.id == spec.id) {
            problems.push(format!("{name}: duplicate rule id '{}'", spec.id));
            continue;
        }
        match CompiledRule::compile(&meta.id, spec, &languages, tables.clone()) {
            Ok(rule) => rules.push(rule),
            Err(errs) => problems.extend(errs.into_iter().map(|e| format!("{name}: {e}"))),
        }
    }
    if !problems.is_empty() {
        return Err(PackError {
            origin: src.origin.clone(),
            problems,
        });
    }
    Ok(Pack {
        meta,
        origin: src.origin.clone(),
        languages,
        rules,
        tables,
    })
}

/// A pack compiled into this binary.
pub struct BuiltinPack {
    pub dir: &'static str,
    pub manifest: &'static str,
    pub rules: &'static [(&'static str, &'static str)],
    pub tables: &'static [(&'static str, &'static str)],
}

include!(concat!(env!("OUT_DIR"), "/builtin_packs.rs"));

/// The sources of every built-in pack.
pub fn builtin_sources() -> Vec<PackSource> {
    BUILTIN_PACKS
        .iter()
        .map(|b| PackSource {
            origin: format!("builtin:{}", b.dir),
            manifest: b.manifest.to_string(),
            rules: b.rules.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect(),
            tables: b.tables.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect(),
        })
        .collect()
}

/// Compile every built-in pack. A broken built-in pack is a bug in this
/// build, caught by the test suite.
pub fn builtin() -> Result<Vec<Pack>, PackError> {
    builtin_sources().iter().map(compile).collect()
}
