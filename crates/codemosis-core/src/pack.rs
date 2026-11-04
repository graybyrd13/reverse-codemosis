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
    pub language: String,
    pub summary: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
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
    pub shape: String,
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
    /// Phrased so that "yes" applies the correction.
    pub question: String,
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CounterexampleSpec {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    pub code: String,
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
    pub language: &'static dyn Language,
    pub rules: Vec<CompiledRule>,
    pub tables: Arc<Tables>,
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
    let Some(language) = lang::by_id(&meta.language) else {
        let known: Vec<&str> = lang::all().iter().map(|l| l.id()).collect();
        return Err(PackError {
            origin: src.origin.clone(),
            problems: vec![format!(
                "pack.toml: unknown language '{}' (this build knows: {})",
                meta.language,
                known.join(", ")
            )],
        });
    };
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
        match CompiledRule::compile(&meta.id, spec, language, tables.clone()) {
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
        language,
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
