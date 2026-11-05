//! # codemosis-engine
//!
//! The part of Reverse Codemosis that changes code: it measures a project
//! against its packs, runs the project's own build and tests as an oracle,
//! and applies corrections one verified step at a time, rolling back any
//! step that would make the code worse.

pub mod answers;
pub mod audit;
pub mod ci;
pub mod config;
pub mod formatter;
pub mod fsguard;
pub mod lock;
pub mod oracle;
pub mod patch;
pub mod policy;
pub mod proof;
pub mod report;
pub mod session;
pub mod testparse;
pub mod vcs;

use answers::Answers;
use codemosis_core::{CompiledRule, Pack, Project, Scan};
use config::{Config, ConfigError, OracleSection};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Project(#[from] codemosis_core::project::ProjectError),
    #[error("{0}")]
    Answers(String),
}

/// A project opened for correction: its configuration, packs, recorded
/// answers and parsed source.
pub struct Workspace {
    pub root: PathBuf,
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub packs: Vec<Pack>,
    pub answers: Answers,
    pub project: Project,
    pub policy: policy::Policy,
}

impl Workspace {
    pub fn open(root: &Path, extra_packs: &[PathBuf]) -> Result<Workspace, EngineError> {
        let t = std::time::Instant::now();
        let (config, config_path) = Config::load(root)?;
        codemosis_core::debug!(
            "configuration: {}",
            config_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "none (defaults)".into())
        );
        let project = Project::load(root, &config.filter())?;
        codemosis_core::debug!(
            "{} file(s) read, {} skipped, in {:.2?} on {} thread(s)",
            project.files.len(),
            project.skipped.len(),
            t.elapsed(),
            codemosis_core::parallel::threads()
        );
        for (p, why) in &project.skipped {
            codemosis_core::trace!("skipped {}: {why}", p.display());
        }
        // Compile only the packs that can apply: text packs, and packs for
        // a grammar the project contains. Every pack still counts for the
        // lockfile and for names in the policy.
        let sources = config.pack_sources(root, extra_packs)?;
        let index: Vec<_> = sources.iter().map(codemosis_core::pack::index).collect();
        let languages = project.languages();
        let wanted: Vec<_> = sources
            .iter()
            .zip(&index)
            .filter(|(_, i)| {
                i.as_ref().is_none_or(|i| {
                    i.languages.is_empty() || i.languages.iter().any(|l| languages.contains(&l.as_str()))
                })
            })
            .map(|(s, _)| s.clone())
            .collect();
        let packs = config::compile_all(&wanted)?;
        codemosis_core::debug!(
            "{} of {} pack(s) apply and were compiled: {}",
            packs.len(),
            sources.len(),
            packs.iter().map(|p| p.meta.id.as_str()).collect::<Vec<_>>().join(", ")
        );
        let answers = Answers::load(root).map_err(EngineError::Answers)?;
        let policy = config.policy()?;
        if let Some(lock) = lock::Lock::load(root).map_err(|e| EngineError::Config(ConfigError::Other(e)))? {
            let ids: Vec<(String, String, String)> = index
                .iter()
                .flatten()
                .map(|i| (i.id.clone(), i.version.clone(), i.digest.clone()))
                .collect();
            let problems = lock.verify_ids(&ids);
            if !problems.is_empty() {
                return Err(EngineError::Config(ConfigError::Other(format!(
                    "the packs do not match {}:\n  - {}\nReview the pack changes, then run `codemosis lock` to pin them again.",
                    lock::LOCK_FILE,
                    problems.join("\n  - ")
                ))));
            }
        }
        let known = |name: &str| {
            index
                .iter()
                .flatten()
                .any(|i| i.id == name || i.rules.iter().any(|r| r == name))
        };
        for name in policy.named_rules() {
            if !known(&name) {
                return Err(EngineError::Config(ConfigError::Other(format!(
                    "{}: '{name}' is not a rule or pack id; `codemosis rules` lists them",
                    config::FILE_NAME
                ))));
            }
        }
        Ok(Workspace {
            root: root.to_path_buf(),
            config,
            config_path,
            packs,
            answers,
            project,
            policy,
        })
    }

    /// Every finding of the enabled rules (or only these), with the
    /// project's suppressions applied.
    pub fn scan_rules(&self, only: &[String]) -> Scan {
        let rules = enabled_rules(&self.packs, &self.config, &self.project, only);
        let answers = |id: &str| self.answers.get(id);
        let mut scan = codemosis_core::scan(&self.project, rules, &answers, None);
        self.policy
            .apply_suppressions(&mut scan.findings, &codemosis_core::suppress::today());
        scan
    }

    pub fn scan(&self) -> Scan {
        self.scan_rules(&[])
    }

    /// Like [`Workspace::scan_rules`], but only in these files (indices into
    /// `project.files`). Facts may still read every file of the project.
    pub fn scan_files(&self, only: &[String], files: &std::collections::BTreeSet<usize>) -> Scan {
        let rules = enabled_rules(&self.packs, &self.config, &self.project, only);
        let answers = |id: &str| self.answers.get(id);
        let mut scan = codemosis_core::scan(&self.project, rules, &answers, Some(files));
        self.policy
            .apply_suppressions(&mut scan.findings, &codemosis_core::suppress::today());
        scan
    }

    /// The configured oracle, or a detected one. The flag says whether it
    /// was detected rather than configured.
    pub fn oracle_section(&self) -> (OracleSection, bool) {
        let o = &self.config.oracle;
        if o.build.is_some() || o.test.is_some() {
            (o.clone(), false)
        } else {
            let d = config::detect_oracle(&self.root);
            let detected = d.build.is_some() || d.test.is_some();
            (d, detected)
        }
    }
}

pub fn oracle_from(root: &Path, section: &OracleSection) -> oracle::Oracle {
    oracle::Oracle::new(
        root,
        section.build.clone(),
        section.test.clone(),
        Duration::from_secs(section.timeout.unwrap_or(600)),
        section.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    )
}

/// The rules that apply: enabled by the configuration, written for a
/// language the project contains, and (if `only` is not empty) selected.
pub fn enabled_rules<'p>(
    packs: &'p [Pack],
    config: &Config,
    project: &Project,
    only: &[String],
) -> Vec<&'p CompiledRule> {
    let languages = project.languages();
    packs
        .iter()
        .filter(|p| p.applies_to(&languages))
        .flat_map(|p| p.rules.iter())
        .filter(|r| config.rule_enabled(&r.id))
        .filter(|r| only.is_empty() || only.iter().any(|o| r.id == *o || r.id.starts_with(&format!("{o}/"))))
        .collect()
}
