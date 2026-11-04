//! Reading individual test results out of test-runner output, so the engine
//! can tell exactly which tests a step would break.
//!
//! Understood formats: Python unittest (`-v`), pytest (`-v` or `-rA`), Rust
//! libtest (`cargo test`) and Go (`go test -v`). When nothing is recognised
//! the engine falls back to the command's exit status.

use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

/// Every test result found in `output`, by test name.
pub fn parse(output: &str) -> BTreeMap<String, TestStatus> {
    static UNITTEST: OnceLock<Regex> = OnceLock::new();
    static PYTEST_SUMMARY: OnceLock<Regex> = OnceLock::new();
    static PYTEST_VERBOSE: OnceLock<Regex> = OnceLock::new();
    static LIBTEST: OnceLock<Regex> = OnceLock::new();
    static LIBTEST_BINARY: OnceLock<Regex> = OnceLock::new();
    static GO: OnceLock<Regex> = OnceLock::new();

    let mut results = BTreeMap::new();

    // unittest -v: "test_x (pkg.mod.Class.test_x) ... ok", optionally with
    // the test's docstring on the following line before the status.
    let unittest = re(
        &UNITTEST,
        r"(?m)^([\w.]+) \(([\w.]+)\)(?:\n[^\n]*?)? \.\.\. (ok|FAIL|ERROR|skipped|expected failure|unexpected success)",
    );
    for c in unittest.captures_iter(output) {
        let method = &c[1];
        let qual = &c[2];
        let name = if qual.ends_with(&format!(".{method}")) {
            qual.to_string()
        } else {
            format!("{qual}.{method}")
        };
        let status = match &c[3] {
            "ok" | "expected failure" => TestStatus::Passed,
            "skipped" => TestStatus::Skipped,
            _ => TestStatus::Failed,
        };
        results.insert(name, status);
    }

    let pytest_status = |s: &str| match s {
        "PASSED" | "XFAIL" => TestStatus::Passed,
        "SKIPPED" => TestStatus::Skipped,
        _ => TestStatus::Failed,
    };
    for c in re(
        &PYTEST_VERBOSE,
        r"(?m)^(\S+::\S+) (PASSED|FAILED|ERROR|SKIPPED|XFAIL|XPASS)",
    )
    .captures_iter(output)
    {
        results.insert(c[1].to_string(), pytest_status(&c[2]));
    }
    for c in re(
        &PYTEST_SUMMARY,
        r"(?m)^(PASSED|FAILED|ERROR|SKIPPED|XFAIL|XPASS) (\S+::\S+)",
    )
    .captures_iter(output)
    {
        results.insert(c[2].to_string(), pytest_status(&c[1]));
    }

    // libtest: names are only unique within one test binary, so prefix them
    // with the binary cargo says it is running.
    let binary = re(&LIBTEST_BINARY, r"^\s*Running (?:unittests )?(\S+)");
    let libtest = re(&LIBTEST, r"^test (\S+) \.\.\. (ok|FAILED|ignored)");
    let mut current = String::new();
    for line in output.lines() {
        if let Some(c) = binary.captures(line) {
            current = c[1].to_string();
            continue;
        }
        if let Some(c) = libtest.captures(line) {
            let name = if current.is_empty() {
                c[1].to_string()
            } else {
                format!("{current}::{}", &c[1])
            };
            let status = match &c[2] {
                "ok" => TestStatus::Passed,
                "ignored" => TestStatus::Skipped,
                _ => TestStatus::Failed,
            };
            results.insert(name, status);
        }
    }

    for c in re(&GO, r"(?m)^\s*--- (PASS|FAIL|SKIP): (\S+)").captures_iter(output) {
        let status = match &c[1] {
            "PASS" => TestStatus::Passed,
            "SKIP" => TestStatus::Skipped,
            _ => TestStatus::Failed,
        };
        results.insert(c[2].to_string(), status);
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unittest_verbose() {
        let out = "\
test_add (tests.test_store.StoreTests.test_add) ... ok
test_remove (tests.test_store.StoreTests.test_remove) ... FAIL
test_doc (tests.test_store.StoreTests)
Checks the docs. ... ERROR
test_skip (tests.test_store.StoreTests.test_skip) ... skipped 'later'
test_store (unittest.loader._FailedTest.test_store) ... ERROR
tests.test_models (unittest.loader._FailedTest.tests.test_models) ... ERROR
";
        let r = parse(out);
        assert_eq!(r["tests.test_store.StoreTests.test_add"], TestStatus::Passed);
        assert_eq!(r["tests.test_store.StoreTests.test_remove"], TestStatus::Failed);
        assert_eq!(r["tests.test_store.StoreTests.test_doc"], TestStatus::Failed);
        assert_eq!(r["tests.test_store.StoreTests.test_skip"], TestStatus::Skipped);
        assert_eq!(r["unittest.loader._FailedTest.test_store"], TestStatus::Failed);
        assert_eq!(r["unittest.loader._FailedTest.tests.test_models"], TestStatus::Failed);
    }

    #[test]
    fn pytest_and_go() {
        let out =
            "tests/test_a.py::test_one PASSED\nFAILED tests/test_a.py::test_two - assert\n--- PASS: TestX (0.00s)\n";
        let r = parse(out);
        assert_eq!(r["tests/test_a.py::test_one"], TestStatus::Passed);
        assert_eq!(r["tests/test_a.py::test_two"], TestStatus::Failed);
        assert_eq!(r["TestX"], TestStatus::Passed);
    }

    #[test]
    fn libtest_prefixes_binaries() {
        let out = "\
     Running unittests src/lib.rs (target/debug/deps/a-1)
test tests::works ... ok
     Running tests/it.rs (target/debug/deps/it-2)
test tests::works ... FAILED
test slow ... ignored
";
        let r = parse(out);
        assert_eq!(r["src/lib.rs::tests::works"], TestStatus::Passed);
        assert_eq!(r["tests/it.rs::tests::works"], TestStatus::Failed);
        assert_eq!(r["tests/it.rs::slow"], TestStatus::Skipped);
    }
}
