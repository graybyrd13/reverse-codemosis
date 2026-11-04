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
        let (config, config_path) = Config::load(root)?;
        let packs = config.load_packs(root, extra_packs)?;
        let answers = Answers::load(root).map_err(EngineError::Answers)?;
        let project = Project::load(root, &config.filter())?;
        let policy = config.policy()?;
        if let Some(lock) = lock::Lock::load(root).map_err(|e| EngineError::Config(ConfigError::Other(e)))? {
            let problems = lock.verify(&packs);
            if !problems.is_empty() {
                return Err(EngineError::Config(ConfigError::Other(format!(
                    "the packs do not match {}:\n  - {}\nReview the pack changes, then run `codemosis lock` to pin them again.",
                    lock::LOCK_FILE,
                    problems.join("\n  - ")
                ))));
            }
        }
        let known = |name: &str| {
            packs
                .iter()
                .any(|p| p.meta.id == name || p.rules.iter().any(|r| r.id == name))
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
