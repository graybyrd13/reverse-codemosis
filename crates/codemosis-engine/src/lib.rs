//! # codemosis-engine
//!
//! The part of Reverse Codemosis that changes code: it measures a project
//! against its packs, runs the project's own build and tests as an oracle,
//! and applies corrections one verified step at a time, rolling back any
//! step that would make the code worse.

pub mod answers;
pub mod config;
pub mod oracle;
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
}

impl Workspace {
    pub fn open(root: &Path, extra_packs: &[PathBuf]) -> Result<Workspace, EngineError> {
        let (config, config_path) = Config::load(root)?;
        let packs = config.load_packs(root, extra_packs)?;
        let answers = Answers::load(root).map_err(EngineError::Answers)?;
        let project = Project::load(root, &config.filter())?;
        Ok(Workspace {
            root: root.to_path_buf(),
            config,
            config_path,
            packs,
            answers,
            project,
        })
    }

    pub fn scan(&self) -> Scan {
        let rules = enabled_rules(&self.packs, &self.config, &self.project, &[]);
        let answers = |id: &str| self.answers.get(id);
        codemosis_core::scan(&self.project, rules, &answers, None)
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
