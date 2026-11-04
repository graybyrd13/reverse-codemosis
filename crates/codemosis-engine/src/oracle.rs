//! The oracle: the project's own build and test commands, run before and
//! after every step. A step is kept only if the oracle says nothing got
//! worse.

use crate::testparse::{self, TestStatus};
use codemosis_core::process::{self, ScratchDir};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
pub struct BuildOutcome {
    pub ok: bool,
    pub errors: usize,
    pub timed_out: bool,
    /// The end of the output, for reports.
    pub tail: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TestOutcome {
    /// The test command exited with status 0.
    pub ok: bool,
    pub timed_out: bool,
    pub results: BTreeMap<String, TestStatus>,
    pub tail: String,
}

impl TestOutcome {
    pub fn count(&self, status: TestStatus) -> usize {
        self.results.values().filter(|s| **s == status).count()
    }
}

/// One run of the oracle.
#[derive(Clone, Debug, Serialize)]
pub struct OracleRun {
    pub build: Option<BuildOutcome>,
    pub tests: Option<TestOutcome>,
    pub seconds: f64,
}

impl OracleRun {
    pub fn passed(&self) -> usize {
        self.tests.as_ref().map(|t| t.count(TestStatus::Passed)).unwrap_or(0)
    }

    pub fn failed(&self) -> usize {
        self.tests.as_ref().map(|t| t.count(TestStatus::Failed)).unwrap_or(0)
    }

    pub fn build_ok(&self) -> Option<bool> {
        self.build.as_ref().map(|b| b.ok)
    }

    /// True when the build (if any) succeeds and the tests (if any) pass.
    pub fn green(&self) -> bool {
        self.build.as_ref().is_none_or(|b| b.ok) && self.tests.as_ref().is_none_or(|t| t.ok && self.failed() == 0)
    }
}

/// Every way `after` is worse than `before`. Empty means no regression.
pub fn regressions(before: &OracleRun, after: &OracleRun) -> Vec<String> {
    let mut out = Vec::new();
    if let (Some(b), Some(a)) = (&before.build, &after.build) {
        if a.timed_out && !b.timed_out {
            out.push("the build timed out".to_string());
        } else if b.ok && !a.ok {
            out.push(format!("the build broke:\n{}", indent(&a.tail)));
        } else if !b.ok && !a.ok && a.errors > b.errors {
            out.push(format!("the build has more errors ({} → {})", b.errors, a.errors));
        }
    }
    if let (Some(b), Some(a)) = (&before.tests, &after.tests) {
        if a.timed_out && !b.timed_out {
            out.push("the test suite timed out".to_string());
            return out;
        }
        let mut lost = Vec::new();
        for (name, status) in &b.results {
            if *status != TestStatus::Passed {
                continue;
            }
            match a.results.get(name) {
                Some(TestStatus::Passed) => {}
                Some(TestStatus::Failed) => lost.push(format!("{name} now fails")),
                Some(TestStatus::Skipped) => lost.push(format!("{name} is now skipped")),
                None => lost.push(format!("{name} no longer runs")),
            }
        }
        if !lost.is_empty() {
            out.push(format!("tests that passed before no longer pass: {}", lost.join(", ")));
        } else if b.ok && !a.ok {
            out.push(format!("the test command now fails:\n{}", indent(&a.tail)));
        }
    }
    out
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n")
}

fn tail(s: &str, lines: usize) -> String {
    let all: Vec<&str> = s.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

fn count_build_errors(output: &str) -> usize {
    // compileall reports each file once with this line, then the error.
    let compileall = output.lines().filter(|l| l.starts_with("*** Error compiling")).count();
    if compileall > 0 {
        return compileall;
    }
    output
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("error") || t.starts_with("*** Error compiling") || t.contains("SyntaxError:")
        })
        .count()
}

/// The build and test commands of a project.
pub struct Oracle {
    pub root: PathBuf,
    pub build: Option<String>,
    pub test: Option<String>,
    pub timeout: Duration,
    pub env: Vec<(String, String)>,
    cache: Option<ScratchDir>,
}

impl Oracle {
    pub fn new(
        root: &Path,
        build: Option<String>,
        test: Option<String>,
        timeout: Duration,
        env: Vec<(String, String)>,
    ) -> Oracle {
        Oracle {
            root: root.to_path_buf(),
            build,
            test,
            timeout,
            env,
            cache: ScratchDir::new("pycache").ok(),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.build.is_some() || self.test.is_some()
    }

    fn command(&self, cmd: &str) -> std::io::Result<process::CommandResult> {
        let mut c = process::shell(cmd);
        c.current_dir(&self.root);
        // Keep Python bytecode out of the project so a run leaves no trace.
        c.env("PYTHONDONTWRITEBYTECODE", "1");
        if let Some(cache) = &self.cache {
            c.env("PYTHONPYCACHEPREFIX", &cache.path);
        }
        for (k, v) in &self.env {
            c.env(k, v);
        }
        process::run(&mut c, self.timeout)
    }

    pub fn run(&self) -> std::io::Result<OracleRun> {
        let start = std::time::Instant::now();
        let build = match &self.build {
            None => None,
            Some(cmd) => {
                let r = self.command(cmd)?;
                let combined = r.combined();
                Some(BuildOutcome {
                    ok: r.success(),
                    errors: if r.success() {
                        0
                    } else {
                        count_build_errors(&combined).max(1)
                    },
                    timed_out: r.timed_out,
                    tail: tail(&combined, 15),
                })
            }
        };
        let tests = match &self.test {
            None => None,
            Some(cmd) => {
                let r = self.command(cmd)?;
                let combined = r.combined();
                Some(TestOutcome {
                    ok: r.success(),
                    timed_out: r.timed_out,
                    results: testparse::parse(&combined),
                    tail: tail(&combined, 25),
                })
            }
        };
        Ok(OracleRun {
            build,
            tests,
            seconds: start.elapsed().as_secs_f64(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(results: &[(&str, TestStatus)], ok: bool) -> OracleRun {
        OracleRun {
            build: Some(BuildOutcome {
                ok: true,
                errors: 0,
                timed_out: false,
                tail: String::new(),
            }),
            tests: Some(TestOutcome {
                ok,
                timed_out: false,
                results: results.iter().map(|(n, s)| (n.to_string(), *s)).collect(),
                tail: String::new(),
            }),
            seconds: 0.0,
        }
    }

    #[test]
    fn fixing_failures_is_not_a_regression() {
        let before = run(&[("a", TestStatus::Passed), ("b", TestStatus::Failed)], false);
        let after = run(&[("a", TestStatus::Passed), ("b", TestStatus::Passed)], true);
        assert!(regressions(&before, &after).is_empty());
    }

    #[test]
    fn losing_a_passing_test_is() {
        let before = run(&[("a", TestStatus::Passed), ("b", TestStatus::Passed)], true);
        let after = run(&[("a", TestStatus::Passed)], false);
        let r = regressions(&before, &after);
        assert_eq!(r.len(), 1);
        assert!(r[0].contains("b no longer runs"));
    }

    #[test]
    fn breaking_the_build_is() {
        let before = run(&[], true);
        let mut after = run(&[], true);
        after.build.as_mut().unwrap().ok = false;
        assert!(!regressions(&before, &after).is_empty());
    }
}
