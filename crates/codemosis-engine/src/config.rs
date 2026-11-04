//! `codemosis.toml`: how a project is built and tested, and which packs and
//! rules apply. Every section is optional.
//!
//! ```toml
//! [project]
//! include = ["src", "tests"]
//! exclude = ["src/generated"]
//!
//! [oracle]
//! build = "python3 -m compileall -q src"
//! test = "python3 -m unittest discover -s tests -v"
//! timeout = 300            # seconds per command
//! env = { PYTHONPATH = "src" }
//!
//! [packs]
//! builtin = ["python-core"] # default: every built-in pack
//! paths = ["../my-packs/company-rules"]
//!
//! [rules]
//! disable = ["python-core/redundant-pass"]
//! ```

use codemosis_core::pack::{self, Pack, PackError};
use codemosis_core::FileFilter;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const FILE_NAME: &str = "codemosis.toml";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub project: ProjectSection,
    pub oracle: OracleSection,
    pub packs: PacksSection,
    pub rules: RulesSection,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ProjectSection {
    pub include: Vec<PathBuf>,
    pub exclude: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct OracleSection {
    pub build: Option<String>,
    pub test: Option<String>,
    pub timeout: Option<u64>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PacksSection {
    pub builtin: Option<Vec<String>>,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RulesSection {
    pub disable: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, std::io::Error),
    #[error("{0} is not valid: {1}")]
    Parse(PathBuf, String),
    #[error(transparent)]
    Pack(#[from] PackError),
    #[error("{0}")]
    Other(String),
}

impl Config {
    /// Read `codemosis.toml` from `root`, or the defaults when it is absent.
    pub fn load(root: &Path) -> Result<(Config, Option<PathBuf>), ConfigError> {
        let path = root.join(FILE_NAME);
        if !path.is_file() {
            return Ok((Config::default(), None));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| ConfigError::Read(path.clone(), e))?;
        let cfg: Config = toml::from_str(&text).map_err(|e| ConfigError::Parse(path.clone(), e.to_string()))?;
        Ok((cfg, Some(path)))
    }

    pub fn filter(&self) -> FileFilter {
        FileFilter {
            include: self.project.include.clone(),
            exclude: self.project.exclude.clone(),
        }
    }

    /// The packs that apply to this project: the selected built-in packs and
    /// any packs listed by path (relative to the project root).
    pub fn load_packs(&self, root: &Path, extra: &[PathBuf]) -> Result<Vec<Pack>, ConfigError> {
        let mut packs = Vec::new();
        for src in pack::builtin_sources() {
            let p = pack::compile(&src)?;
            let wanted = match &self.packs.builtin {
                None => true,
                Some(list) => list.contains(&p.meta.id),
            };
            if wanted {
                packs.push(p);
            }
        }
        if let Some(list) = &self.packs.builtin {
            for id in list {
                if !packs.iter().any(|p| p.meta.id == *id) {
                    return Err(ConfigError::Other(format!("there is no built-in pack named '{id}'")));
                }
            }
        }
        for dir in self
            .packs
            .paths
            .iter()
            .map(|p| root.join(p))
            .chain(extra.iter().cloned())
        {
            let p = pack::compile(&pack::read_dir(&dir)?)?;
            if let Some(i) = packs.iter().position(|q| q.meta.id == p.meta.id) {
                packs.remove(i);
            }
            packs.push(p);
        }
        Ok(packs)
    }

    pub fn rule_enabled(&self, id: &str) -> bool {
        !self
            .rules
            .disable
            .iter()
            .any(|d| d == id || id.starts_with(&format!("{d}/")))
    }
}

/// Guess the oracle of a project that has no `[oracle]` section.
pub fn detect_oracle(root: &Path) -> OracleSection {
    let mut o = OracleSection::default();
    if root.join("Cargo.toml").is_file() {
        o.build = Some("cargo build --all-targets".to_string());
        o.test = Some("cargo test".to_string());
        return o;
    }
    let has_python = ["setup.py", "pyproject.toml", "setup.cfg", "requirements.txt"]
        .iter()
        .any(|f| root.join(f).is_file())
        || std::fs::read_dir(root)
            .map(|rd| {
                rd.flatten().any(|e| {
                    let p = e.path();
                    p.extension().is_some_and(|x| x == "py") || (p.is_dir() && p.join("__init__.py").is_file())
                })
            })
            .unwrap_or(false)
        || root.join("src").is_dir()
        || root.join("tests").is_dir();
    if !has_python {
        return o;
    }
    let python = codemosis_core::lang::python::python_command();
    let src = if root.join("src").is_dir() { "src" } else { "." };
    o.build = Some(format!("{python} -m compileall -q {src}"));
    if src == "src" {
        o.env.insert("PYTHONPATH".to_string(), "src".to_string());
    }
    let uses_pytest = root.join("pytest.ini").is_file()
        || root.join("conftest.py").is_file()
        || std::fs::read_to_string(root.join("pyproject.toml"))
            .map(|t| t.contains("[tool.pytest"))
            .unwrap_or(false);
    o.test = Some(if uses_pytest {
        format!("{python} -m pytest -rA")
    } else if root.join("tests").is_dir() {
        format!("{python} -m unittest discover -s tests -t . -v")
    } else {
        format!("{python} -m unittest discover -v")
    });
    o
}
