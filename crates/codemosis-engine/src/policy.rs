//! What a project decides about findings: which to suppress, how severe
//! each one is, which existing ones are accepted for now (the baseline),
//! and when a check fails.
//!
//! ```toml
//! [policy]
//! fail_on = "warning"                    # error | warning | note | never, or a category
//! baseline = ".codemosis/baseline.json"  # accept these existing findings for now
//!
//! [severity]                             # rule id, pack id or category = error | warning | note
//! "python-core/redundant-pass" = "warning"
//! "universal" = "error"
//! waste = "warning"
//!
//! [[suppress]]
//! rule = "python-core/bare-except"       # rule id, pack id or "*"
//! paths = ["legacy/**"]                  # globs; default every file
//! reason = "legacy worker, replaced in Q1"
//! until = "2027-01-31"                   # optional: after this day it stops applying
//! owner = "@payments"                    # optional
//! ```

use codemosis_core::model::{Category, Evidence, Finding, Status, Suppression, SuppressionKind};
use codemosis_core::suppress::still_applies;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// How loudly a finding is reported, in the SARIF sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Note,
    Warning,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Note => "note",
            Level::Warning => "warning",
            Level::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Level> {
        match s {
            "note" => Some(Level::Note),
            "warning" => Some(Level::Warning),
            "error" => Some(Level::Error),
            _ => None,
        }
    }

    /// The default level of a category.
    pub fn of(category: Category) -> Level {
        match category {
            Category::Broken | Category::Contradiction => Level::Error,
            Category::Hazard => Level::Warning,
            Category::Waste | Category::Simplification => Level::Note,
        }
    }
}

/// When `codemosis check` exits non-zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailOn {
    /// Any open finding at this level or above.
    Level(Level),
    /// Any open finding in this category or a more serious one.
    Category(Category),
    Never,
}

impl FailOn {
    pub fn parse(s: &str) -> Result<FailOn, String> {
        if s == "never" {
            return Ok(FailOn::Never);
        }
        if s == "any" {
            return Ok(FailOn::Level(Level::Note));
        }
        if let Some(l) = Level::parse(s) {
            return Ok(FailOn::Level(l));
        }
        if let Some(c) = Category::parse(s) {
            return Ok(FailOn::Category(c));
        }
        Err(format!(
            "'{s}' is not a threshold: use error, warning, note, any or never, or a category (broken, contradiction, hazard, waste, simplification)"
        ))
    }

    pub fn trips(&self, level: Level, category: Category) -> bool {
        match self {
            FailOn::Never => false,
            FailOn::Level(l) => level >= *l,
            FailOn::Category(c) => category <= *c,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PolicySection {
    /// The threshold `codemosis check` fails on (default: any open finding).
    pub fail_on: Option<String>,
    /// A baseline file of accepted findings, relative to the project root.
    pub baseline: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuppressEntry {
    pub rule: String,
    #[serde(default)]
    pub paths: Vec<String>,
    pub reason: String,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
}

/// The compiled policy of a project.
#[derive(Debug, Clone)]
pub struct Policy {
    pub fail_on: FailOn,
    severity: BTreeMap<String, Level>,
    suppress: Vec<(SuppressEntry, Option<globset::GlobSet>)>,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            fail_on: FailOn::Level(Level::Note),
            severity: BTreeMap::new(),
            suppress: Vec::new(),
        }
    }
}

fn rule_matches(pattern: &str, rule: &str) -> bool {
    pattern == "*" || pattern == rule || rule.starts_with(&format!("{pattern}/"))
}

impl Policy {
    pub fn new(
        section: &PolicySection,
        severity: &BTreeMap<String, String>,
        suppress: &[SuppressEntry],
    ) -> Result<Policy, String> {
        let fail_on = match &section.fail_on {
            Some(s) => FailOn::parse(s).map_err(|e| format!("[policy] fail_on: {e}"))?,
            None => FailOn::Level(Level::Note),
        };
        let mut levels = BTreeMap::new();
        for (key, value) in severity {
            let level = Level::parse(value)
                .ok_or_else(|| format!("[severity] {key}: '{value}' is not error, warning or note"))?;
            levels.insert(key.clone(), level);
        }
        let mut compiled = Vec::new();
        for (i, e) in suppress.iter().enumerate() {
            if e.reason.trim().is_empty() {
                return Err(format!(
                    "[[suppress]] #{}: give a reason, so a reviewer knows why",
                    i + 1
                ));
            }
            if let Err(why) = still_applies(e.until.as_deref(), "0000-00-00") {
                if why.contains("not YYYY-MM-DD") {
                    return Err(format!("[[suppress]] #{}: {why}", i + 1));
                }
            }
            let set = if e.paths.is_empty() {
                None
            } else {
                let mut b = globset::GlobSetBuilder::new();
                for p in &e.paths {
                    b.add(
                        globset::GlobBuilder::new(p)
                            .literal_separator(true)
                            .build()
                            .map_err(|err| format!("[[suppress]] #{}: path '{p}': {err}", i + 1))?,
                    );
                }
                Some(b.build().map_err(|err| format!("[[suppress]] #{}: {err}", i + 1))?)
            };
            compiled.push((e.clone(), set));
        }
        Ok(Policy {
            fail_on,
            severity: levels,
            suppress: compiled,
        })
    }

    /// Rule and pack ids named by `[severity]` and `[[suppress]]`, so the
    /// workspace can report names that match no rule.
    pub fn named_rules(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> = self
            .severity
            .keys()
            .filter(|k| Category::parse(k).is_none())
            .cloned()
            .collect();
        out.extend(self.suppress.iter().map(|(e, _)| e.rule.clone()).filter(|r| r != "*"));
        out
    }

    /// The reporting level of a finding: an exact rule entry, then its pack,
    /// then its category, then the category's default.
    pub fn level(&self, rule: &str, category: Category) -> Level {
        if let Some(l) = self.severity.get(rule) {
            return *l;
        }
        if let Some((pack, _)) = rule.split_once('/') {
            if let Some(l) = self.severity.get(pack) {
                return *l;
            }
        }
        self.severity
            .get(category.as_str())
            .copied()
            .unwrap_or_else(|| Level::of(category))
    }

    /// Mark findings covered by a `[[suppress]]` entry.
    pub fn apply_suppressions(&self, findings: &mut [Finding], today: &str) {
        for f in findings.iter_mut() {
            if !f.is_open() {
                continue;
            }
            let path = f.file.to_string_lossy().replace('\\', "/");
            let Some((entry, _)) = self.suppress.iter().find(|(e, set)| {
                rule_matches(&e.rule, &f.rule) && set.as_ref().is_none_or(|s| s.is_match(Path::new(&path)))
            }) else {
                continue;
            };
            match still_applies(entry.until.as_deref(), today) {
                Ok(()) => {
                    f.status = Status::Suppressed {
                        suppression: Suppression {
                            kind: SuppressionKind::Config,
                            reason: Some(entry.reason.clone()),
                            until: entry.until.clone(),
                            owner: entry.owner.clone(),
                            at: crate::config::FILE_NAME.to_string(),
                        },
                    };
                    f.edits.clear();
                }
                Err(why) => f.evidence.push(Evidence {
                    span: f.span.clone(),
                    label: format!(
                        "the [[suppress]] entry for {} in {} no longer applies: {why}",
                        entry.rule,
                        crate::config::FILE_NAME
                    ),
                }),
            }
        }
    }

    /// Whether these findings fail a check: any open finding that is not in
    /// the baseline and reaches the threshold.
    pub fn fails(&self, findings: &[&Finding], fail_on: FailOn) -> bool {
        findings
            .iter()
            .any(|f| f.is_open() && fail_on.trips(self.level(&f.rule, f.category), f.category))
    }
}

/// Accepted findings: a check reports and fails only on findings that are
/// not in it, so a team can adopt the tool on an existing codebase and hold
/// the line from today.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Baseline {
    pub schema: String,
    pub created: String,
    pub findings: Vec<BaselineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaselineEntry {
    pub fingerprint: String,
    pub rule: String,
    pub file: String,
    pub message: String,
}

pub const BASELINE_SCHEMA: &str = "codemosis/baseline/v1";
pub const DEFAULT_BASELINE: &str = ".codemosis/baseline.json";

impl Baseline {
    pub fn from_findings<'a>(findings: impl IntoIterator<Item = &'a Finding>, today: &str) -> Baseline {
        let mut entries: Vec<BaselineEntry> = findings
            .into_iter()
            .filter(|f| f.is_open())
            .map(|f| BaselineEntry {
                fingerprint: f.fingerprint(),
                rule: f.rule.clone(),
                file: f.file.to_string_lossy().replace('\\', "/"),
                message: f.message.clone(),
            })
            .collect();
        entries.sort_by(|a, b| (&a.file, &a.rule, &a.fingerprint).cmp(&(&b.file, &b.rule, &b.fingerprint)));
        entries.dedup_by(|a, b| a.fingerprint == b.fingerprint);
        Baseline {
            schema: BASELINE_SCHEMA.to_string(),
            created: today.to_string(),
            findings: entries,
        }
    }

    pub fn load(path: &Path) -> Result<Baseline, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let b: Baseline =
            serde_json::from_str(&text).map_err(|e| format!("{} is not a baseline: {e}", path.display()))?;
        if b.schema != BASELINE_SCHEMA {
            return Err(format!(
                "{} has schema '{}'; this version reads '{BASELINE_SCHEMA}'",
                path.display(),
                b.schema
            ));
        }
        Ok(b)
    }

    pub fn contains(&self, f: &Finding) -> bool {
        let fp = f.fingerprint();
        self.findings.iter().any(|e| e.fingerprint == fp)
    }

    /// Entries no current finding matches: fixed since the baseline was made.
    pub fn resolved<'a>(&'a self, findings: &[Finding]) -> Vec<&'a BaselineEntry> {
        let current: BTreeSet<String> = findings.iter().map(|f| f.fingerprint()).collect();
        self.findings
            .iter()
            .filter(|e| !current.contains(&e.fingerprint))
            .collect()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serializable") + "\n"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds() {
        let p = Policy::default();
        assert_eq!(p.level("x/y", Category::Waste), Level::Note);
        assert!(FailOn::parse("warning").unwrap().trips(Level::Error, Category::Broken));
        assert!(!FailOn::parse("warning").unwrap().trips(Level::Note, Category::Waste));
        assert!(FailOn::parse("hazard")
            .unwrap()
            .trips(Level::Note, Category::Contradiction));
        assert!(!FailOn::parse("hazard").unwrap().trips(Level::Error, Category::Waste));
        assert!(!FailOn::parse("never").unwrap().trips(Level::Error, Category::Broken));
        assert!(FailOn::parse("loud").is_err());
    }

    #[test]
    fn severity_overrides_in_order() {
        let sev: BTreeMap<String, String> = [
            ("python-core/redundant-pass", "error"),
            ("python-core", "warning"),
            ("waste", "warning"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let p = Policy::new(&PolicySection::default(), &sev, &[]).unwrap();
        assert_eq!(
            p.level("python-core/redundant-pass", Category::Simplification),
            Level::Error
        );
        assert_eq!(
            p.level("python-core/is-literal", Category::Contradiction),
            Level::Warning
        );
        assert_eq!(p.level("rust-core/needless-return", Category::Waste), Level::Warning);
        assert_eq!(p.level("rust-core/x", Category::Broken), Level::Error);
    }

    #[test]
    fn suppressions_need_reasons_and_valid_dates() {
        let entry = |reason: &str, until: Option<&str>| SuppressEntry {
            rule: "*".into(),
            paths: vec![],
            reason: reason.into(),
            until: until.map(String::from),
            owner: None,
        };
        let none = BTreeMap::new();
        assert!(Policy::new(&PolicySection::default(), &none, &[entry("", None)]).is_err());
        assert!(Policy::new(&PolicySection::default(), &none, &[entry("ok", Some("soon"))]).is_err());
        assert!(Policy::new(&PolicySection::default(), &none, &[entry("ok", Some("2030-01-01"))]).is_ok());
    }
}
