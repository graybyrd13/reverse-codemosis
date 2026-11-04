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

/// Guess the oracle of a project that has no `[oracle]` section, from the
/// build files at its root. The first ecosystem found, in this order, wins:
/// Cargo, Go, npm (pnpm, yarn, bun), Maven, Gradle, .NET, Mix, SwiftPM,
/// Bundler, Composer, Python, Make.
pub fn detect_oracle(root: &Path) -> OracleSection {
    let mut o = OracleSection::default();
    let has = |f: &str| root.join(f).is_file();
    let set = |o: &mut OracleSection, build: Option<&str>, test: Option<String>| {
        o.build = build.map(|b| b.to_string());
        o.test = test;
    };
    if has("Cargo.toml") {
        set(&mut o, Some("cargo build --all-targets"), Some("cargo test".into()));
        return o;
    }
    if has("go.mod") {
        set(&mut o, Some("go build ./..."), Some("go test -v ./...".into()));
        return o;
    }
    if has("package.json") {
        let scripts: serde_json::Map<String, serde_json::Value> = std::fs::read_to_string(root.join("package.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("scripts").and_then(|s| s.as_object()).cloned())
            .unwrap_or_default();
        let runner = if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "yarn"
        } else if has("bun.lockb") || has("bun.lock") {
            "bun"
        } else {
            "npm"
        };
        let real_test = scripts
            .get("test")
            .and_then(|t| t.as_str())
            .is_some_and(|t| !t.contains("no test specified"));
        o.test = real_test.then(|| format!("{runner} test"));
        o.build = scripts.contains_key("build").then(|| format!("{runner} run build"));
        return o;
    }
    if has("pom.xml") {
        set(&mut o, Some("mvn -q -B compile"), Some("mvn -q -B test".into()));
        return o;
    }
    if has("build.gradle") || has("build.gradle.kts") {
        let g = if has("gradlew") { "./gradlew" } else { "gradle" };
        o.build = Some(format!("{g} assemble"));
        o.test = Some(format!("{g} test"));
        return o;
    }
    let dotnet = std::fs::read_dir(root)
        .map(|rd| {
            rd.flatten().any(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.ends_with(".sln") || n.ends_with(".csproj") || n.ends_with(".fsproj")
            })
        })
        .unwrap_or(false);
    if dotnet {
        set(
            &mut o,
            Some("dotnet build"),
            Some("dotnet test --logger \"console;verbosity=normal\"".into()),
        );
        return o;
    }
    if has("mix.exs") {
        set(&mut o, Some("mix compile"), Some("mix test".into()));
        return o;
    }
    if has("Package.swift") {
        set(&mut o, Some("swift build"), Some("swift test".into()));
        return o;
    }
    if has("Gemfile") {
        let test = if root.join("spec").is_dir() {
            Some("bundle exec rspec".to_string())
        } else if has("Rakefile") {
            Some("bundle exec rake test".to_string())
        } else {
            None
        };
        set(&mut o, None, test);
        return o;
    }
    if has("composer.json") {
        let scripted = std::fs::read_to_string(root.join("composer.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .is_some_and(|v| v.pointer("/scripts/test").is_some());
        let test = if scripted {
            Some("composer test".to_string())
        } else if has("vendor/bin/phpunit") {
            Some("vendor/bin/phpunit".to_string())
        } else {
            None
        };
        set(&mut o, None, test);
        return o;
    }
    let python = detect_python(root);
    if python.build.is_some() || python.test.is_some() {
        return python;
    }
    if has("Makefile") {
        let text = std::fs::read_to_string(root.join("Makefile")).unwrap_or_default();
        let has_target = |t: &str| text.lines().any(|l| l.starts_with(&format!("{t}:")));
        o.build = Some("make".to_string());
        o.test = has_target("test").then(|| "make test".to_string());
    }
    o
}

fn detect_python(root: &Path) -> OracleSection {
    let mut o = OracleSection::default();
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
