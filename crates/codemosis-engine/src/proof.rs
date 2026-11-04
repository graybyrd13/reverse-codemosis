//! Proof certificates.
//!
//! Every `codemosis fix` that changes code writes a certificate to
//! `.codemosis/proofs/`: which packs (by content hash) and which human
//! answers justified each step, the exact patch of each step, the hashes of
//! every file before and after, and what the project's build and tests said.
//!
//! `codemosis proof verify` checks a certificate without trusting whoever
//! produced it: it recovers the code as it was before from the current files
//! and the patch, then replays every step with the pinned rules and checks
//! that each one reproduces exactly the recorded change. A certificate that
//! verifies says: this diff is precisely what these reviewed rules prove,
//! given these recorded answers.

use crate::answers::Answers;
use crate::oracle::OracleRun;
use crate::patch;
use crate::session::{unified_diff, Report, StepOracle};
use codemosis_core::{edit, Edit, Pack, Project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const PROOF_SCHEMA: &str = "codemosis/proof/v1";
pub const PROOF_DIR: &str = ".codemosis/proofs";

pub fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The current time in UTC, as `2026-11-04T12:34:56Z`.
pub fn now_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let day = codemosis_core::suppress::civil_date(secs / 86_400);
    let s = secs % 86_400;
    format!("{day}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Certificate {
    pub schema: String,
    pub created: String,
    pub engine: EngineInfo,
    pub environment: Environment,
    /// The packs whose rules took a step, pinned by content.
    pub packs: Vec<PackRef>,
    /// The human answers the steps depend on: question id to answer.
    pub answers: BTreeMap<String, String>,
    pub oracle: Option<OracleRecord>,
    pub steps: Vec<StepRecord>,
    /// Every changed file, before the first step and after the last.
    pub files: Vec<FileRecord>,
    /// The whole change as one exact unified diff.
    pub patch: String,
    /// SHA-256 of this certificate with `digest` empty: tamper evidence.
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EngineInfo {
    pub version: String,
    pub tree_sitter: Option<String>,
    /// Grammar crate versions of the languages the steps touched.
    pub grammars: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Environment {
    pub os: String,
    pub arch: String,
    /// The commit the corrections were made on, when the project is in git.
    pub commit: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PackRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OracleSummary {
    pub build_ok: Option<bool>,
    pub passed: usize,
    pub failed: usize,
}

impl OracleSummary {
    fn of(run: &OracleRun) -> OracleSummary {
        OracleSummary {
            build_ok: run.build_ok(),
            passed: run.passed(),
            failed: run.failed(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OracleRecord {
    pub build: Option<String>,
    pub test: Option<String>,
    pub before: Option<OracleSummary>,
    pub after: Option<OracleSummary>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FindingRef {
    pub fingerprint: String,
    pub file: String,
    pub line: usize,
    /// The question whose answer unlocked this correction, if any.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub question: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileRecord {
    pub path: String,
    pub before_sha256: String,
    pub after_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StepRecord {
    pub number: usize,
    pub rule: String,
    pub pack_sha256: String,
    pub semantics: Option<String>,
    pub category: String,
    pub findings: Vec<FindingRef>,
    pub files: Vec<FileRecord>,
    pub patch: String,
    pub oracle: Option<StepOracleRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StepOracleRecord {
    pub build_ok: Option<bool>,
    pub passed_before: usize,
    pub passed_after: usize,
    pub failed_after: usize,
}

impl From<&StepOracle> for StepOracleRecord {
    fn from(o: &StepOracle) -> Self {
        StepOracleRecord {
            build_ok: o.build_ok,
            passed_before: o.passed_before,
            passed_after: o.passed_after,
            failed_after: o.failed_after,
        }
    }
}

fn git_commit(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl Certificate {
    /// The certificate of a session that changed code, or None if it changed nothing.
    pub fn of(
        report: &Report,
        root: &Path,
        project: &Project,
        packs: &[Pack],
        answers: &Answers,
        oracle: Option<(&Option<String>, &Option<String>)>,
    ) -> Option<Certificate> {
        if report.steps.is_empty() {
            return None;
        }
        let mut first_before: BTreeMap<String, String> = BTreeMap::new();
        let mut last_after: BTreeMap<String, String> = BTreeMap::new();
        let mut steps = Vec::new();
        let mut used_packs: BTreeSet<String> = BTreeSet::new();
        let mut used_answers = BTreeMap::new();
        let mut grammars = BTreeMap::new();
        for st in &report.steps {
            let pack_id = st.rule.split('/').next().unwrap_or("");
            let pack = packs.iter().find(|p| p.meta.id == pack_id);
            used_packs.insert(pack_id.to_string());
            let mut files = Vec::new();
            let mut step_patch = String::new();
            for c in &st.changes {
                first_before.entry(c.path.clone()).or_insert_with(|| c.before.clone());
                last_after.insert(c.path.clone(), c.after.clone());
                files.push(FileRecord {
                    path: c.path.clone(),
                    before_sha256: sha256(&c.before),
                    after_sha256: sha256(&c.after),
                });
                step_patch.push_str(&unified_diff(&c.path, &c.before, &c.after));
                if let Some(l) = project
                    .files
                    .iter()
                    .find(|f| f.display_path() == c.path)
                    .and_then(|f| f.lang)
                {
                    if let Some(v) = codemosis_core::lang::grammar_version(l.id()) {
                        grammars.insert(l.id().to_string(), v.to_string());
                    }
                }
            }
            let findings = st
                .findings
                .iter()
                .map(|f| {
                    let file = f.file.to_string_lossy().replace('\\', "/");
                    let question = f
                        .answered
                        .then(|| codemosis_core::rule::question_id(&f.rule, &file, &f.anchor));
                    if let Some(q) = &question {
                        if let Some(a) = answers.get(q) {
                            used_answers.insert(q.clone(), a);
                        }
                    }
                    FindingRef {
                        fingerprint: f.fingerprint(),
                        file,
                        line: f.span.start.line,
                        question,
                    }
                })
                .collect();
            steps.push(StepRecord {
                number: st.number,
                rule: st.rule.clone(),
                pack_sha256: pack.map(|p| p.digest.clone()).unwrap_or_default(),
                semantics: st.semantics.map(|s| s.as_str().to_string()),
                category: st.category.as_str().to_string(),
                findings,
                files,
                patch: step_patch,
                oracle: st.oracle.as_ref().map(StepOracleRecord::from),
            });
        }
        let mut files = Vec::new();
        let mut whole = String::new();
        for (path, before) in &first_before {
            let after = &last_after[path];
            files.push(FileRecord {
                path: path.clone(),
                before_sha256: sha256(before),
                after_sha256: sha256(after),
            });
            whole.push_str(&unified_diff(path, before, after));
        }
        let mut cert = Certificate {
            schema: PROOF_SCHEMA.to_string(),
            created: now_utc(),
            engine: EngineInfo {
                version: codemosis_core::version::ENGINE_VERSION.to_string(),
                tree_sitter: codemosis_core::lang::runtime_version().map(String::from),
                grammars,
            },
            environment: Environment {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                commit: git_commit(root),
            },
            packs: packs
                .iter()
                .filter(|p| used_packs.contains(&p.meta.id))
                .map(|p| PackRef {
                    id: p.meta.id.clone(),
                    version: p.meta.version.clone(),
                    sha256: p.digest.clone(),
                })
                .collect(),
            answers: used_answers,
            oracle: oracle.map(|(build, test)| OracleRecord {
                build: build.clone(),
                test: test.clone(),
                before: report.baseline.as_ref().map(OracleSummary::of),
                after: report.result.as_ref().map(OracleSummary::of),
            }),
            steps,
            files,
            patch: whole,
            digest: String::new(),
        };
        cert.digest = cert.compute_digest();
        Some(cert)
    }

    pub fn compute_digest(&self) -> String {
        let mut c = self.clone();
        c.digest = String::new();
        sha256(&serde_json::to_string(&c).expect("serializable"))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serializable") + "\n"
    }

    pub fn file_name(&self) -> PathBuf {
        let day = self.created.get(..10).unwrap_or("undated");
        Path::new(PROOF_DIR).join(format!("{day}-{}.json", &self.digest[..12]))
    }

    pub fn load(path: &Path) -> Result<Certificate, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let c: Certificate =
            serde_json::from_str(&text).map_err(|e| format!("{} is not a proof certificate: {e}", path.display()))?;
        if c.schema != PROOF_SCHEMA {
            return Err(format!(
                "{} has schema '{}'; this engine reads '{PROOF_SCHEMA}'",
                path.display(),
                c.schema
            ));
        }
        Ok(c)
    }
}

/// The outcome of verifying a certificate: every check, in order.
#[derive(Debug, Default)]
pub struct Verification {
    pub checks: Vec<(String, Result<(), String>)>,
}

impl Verification {
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|(_, r)| r.is_ok())
    }

    fn push(&mut self, name: impl Into<String>, r: Result<(), String>) -> bool {
        let ok = r.is_ok();
        self.checks.push((name.into(), r));
        ok
    }
}

/// Verify a certificate against the project at `root` (whose files must be
/// as the certificate left them) and the packs available now.
pub fn verify(cert: &Certificate, root: &Path, project: &mut Project, packs: &[Pack]) -> Verification {
    let mut v = Verification::default();
    let digest = cert.compute_digest();
    if !v.push(
        "the certificate is intact (its digest matches its content)",
        if digest == cert.digest {
            Ok(())
        } else {
            Err(format!("digest is {}, expected {}", digest, cert.digest))
        },
    ) {
        return v;
    }

    // Every changed file is as the certificate left it.
    let mut current: BTreeMap<String, String> = BTreeMap::new();
    let mut ok = true;
    for f in &cert.files {
        let text = std::fs::read_to_string(root.join(&f.path)).unwrap_or_default();
        let r = if sha256(&text) == f.after_sha256 {
            Ok(())
        } else {
            Err("the file changed since the certificate was written".to_string())
        };
        ok &= v.push(format!("{} is as the correction left it", f.path), r);
        current.insert(f.path.clone(), text);
    }
    if !ok {
        return v;
    }

    // Recover the code as it was before, from the patch.
    let patches = match patch::parse(&cert.patch) {
        Ok(p) => p,
        Err(e) => {
            v.push("the patch is well formed", Err(e));
            return v;
        }
    };
    let mut before: BTreeMap<String, String> = BTreeMap::new();
    for f in &cert.files {
        let r = patches
            .iter()
            .find(|p| p.path == f.path)
            .ok_or_else(|| "the patch does not cover this file".to_string())
            .and_then(|p| patch::reverse(&current[&f.path], p))
            .and_then(|text| {
                if sha256(&text) == f.before_sha256 {
                    before.insert(f.path.clone(), text);
                    Ok(())
                } else {
                    Err("reversing the patch does not give the recorded original".to_string())
                }
            });
        if !v.push(format!("{}: the original is recovered from the patch", f.path), r) {
            return v;
        }
    }

    // Replay every step with the pinned rules.
    for (path, text) in &before {
        match project.index_of(Path::new(path)) {
            Some(i) => project.set_text(i, text.clone()),
            None => {
                v.push(format!("{path} is part of the project"), Err("not found".into()));
                return v;
            }
        }
    }
    let answers = |id: &str| cert.answers.get(id).cloned();
    for st in &cert.steps {
        let name = format!("step {} ({}) reproduces exactly", st.number, st.rule);
        let Some(pack) = packs.iter().find(|p| p.digest == st.pack_sha256) else {
            v.push(
                name,
                Err(format!(
                    "no available pack has sha256 {}…; the rule cannot be replayed with this engine's packs",
                    &st.pack_sha256[..12.min(st.pack_sha256.len())]
                )),
            );
            return v;
        };
        let Some(rule) = pack.rules.iter().find(|r| r.id == st.rule) else {
            v.push(name, Err(format!("pack {} has no rule {}", pack.meta.id, st.rule)));
            return v;
        };
        let scan = codemosis_core::scan(project, [rule], &answers, None);
        let mut per_file: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
        let mut missing = Vec::new();
        for fr in &st.findings {
            match scan
                .findings
                .iter()
                .find(|f| f.fingerprint() == fr.fingerprint && f.is_fixable())
            {
                Some(f) => per_file
                    .entry(fr.file.clone())
                    .or_default()
                    .extend(f.edits.iter().cloned()),
                None => missing.push(format!("{}:{}", fr.file, fr.line)),
            }
        }
        if !missing.is_empty() {
            v.push(
                name,
                Err(format!(
                    "the rule no longer proves the correction at {}",
                    missing.join(", ")
                )),
            );
            return v;
        }
        let mut problems = Vec::new();
        for rec in &st.files {
            let Some(idx) = project.index_of(Path::new(&rec.path)) else {
                problems.push(format!("{} is not in the project", rec.path));
                continue;
            };
            let old = project.files[idx].text.clone();
            if sha256(&old) != rec.before_sha256 {
                problems.push(format!("{} is not in the state the step started from", rec.path));
                continue;
            }
            let edits = per_file.remove(&rec.path).unwrap_or_default();
            match edit::apply(&old, &edits) {
                Ok(new) if sha256(&new) == rec.after_sha256 => project.set_text(idx, new),
                Ok(_) => problems.push(format!(
                    "{}: the replayed correction differs from the recorded one",
                    rec.path
                )),
                Err(e) => problems.push(format!("{}: {e}", rec.path)),
            }
        }
        if !problems.is_empty() || !per_file.is_empty() {
            problems.extend(per_file.keys().map(|p| format!("{p}: the rule also changes this file")));
            v.push(name, Err(problems.join("; ")));
            return v;
        }
        v.push(name, Ok(()));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_iso() {
        let t = now_utc();
        assert_eq!(t.len(), 20, "{t}");
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
    }
}
