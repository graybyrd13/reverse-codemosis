//! Reading individual test results out of test-runner output, so the engine
//! can tell exactly which tests a step would break.
//!
//! Understood formats: Python unittest (`-v`), pytest (`-v` or `-rA`), Rust
//! libtest (`cargo test`), Go (`go test -v`), TAP (Node's test runner when
//! piped, tap, prove), Jest / Vitest / Mocha / node:test spec output, and
//! `dotnet test` with normal verbosity. When nothing is recognised the
//! engine falls back to the command's exit status.

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

    // TAP: "ok 1 - name", "not ok 2 - name", "ok 3 - name # SKIP". Nested
    // subtests are indented; their names are prefixed with the parent's.
    static TAP: OnceLock<Regex> = OnceLock::new();
    let tap = re(
        &TAP,
        r"^(\s*)(not ok|ok) \d+(?: -)? ([^#\n]*?)\s*(#\s*(SKIP|TODO)\b.*)?$",
    );
    if output
        .lines()
        .any(|l| l.starts_with("TAP version") || l.trim_start().starts_with("1.."))
    {
        let mut pending: Vec<(usize, String, TestStatus)> = Vec::new();
        for line in output.lines() {
            let Some(c) = tap.captures(line) else { continue };
            let depth = c[1].len();
            let status = if c.get(4).is_some() {
                TestStatus::Skipped
            } else if &c[2] == "ok" {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            pending.push((depth, c[3].trim().to_string(), status));
        }
        // A parent's result line follows its subtests, so walk backwards:
        // a line followed (in reverse, preceded) by deeper lines is a parent.
        let mut parents: Vec<(usize, String)> = Vec::new();
        for i in (0..pending.len()).rev() {
            let (depth, name, status) = &pending[i];
            while parents.last().is_some_and(|(d, _)| d >= depth) {
                parents.pop();
            }
            let is_parent = i > 0 && pending[i - 1].0 > *depth;
            if is_parent {
                parents.push((*depth, name.clone()));
            } else {
                let full = parents
                    .iter()
                    .map(|(_, n)| n.as_str())
                    .chain([name.as_str()])
                    .collect::<Vec<_>>()
                    .join(" > ");
                results.insert(full, *status);
            }
        }
    }

    // Spec reporters: "✓ name", "✔ name (1ms)", "✕ name", "✖ name", "× name".
    static SPEC: OnceLock<Regex> = OnceLock::new();
    static SPEC_FILE: OnceLock<Regex> = OnceLock::new();
    let spec = re(&SPEC, r"^\s*([✓✔√✕✖×])\s+(.+?)(?:\s+\(\d+(?:\.\d+)?\s*m?s\))?\s*$");
    let spec_file = re(&SPEC_FILE, r"^\s*(?:PASS|FAIL)\s+(\S+)");
    let mut file = String::new();
    for line in output.lines() {
        if let Some(c) = spec_file.captures(line) {
            file = c[1].to_string();
            continue;
        }
        if let Some(c) = spec.captures(line) {
            let status = if matches!(&c[1], "✓" | "✔" | "√") {
                TestStatus::Passed
            } else {
                TestStatus::Failed
            };
            let name = if file.is_empty() {
                c[2].to_string()
            } else {
                format!("{file} > {}", &c[2])
            };
            results.entry(name).or_insert(status);
        }
    }

    // dotnet test --logger "console;verbosity=normal"
    static DOTNET: OnceLock<Regex> = OnceLock::new();
    for c in re(
        &DOTNET,
        r"(?m)^\s+(Passed|Failed|Skipped) ([\w.+`<>,]+)(?: \[[^\]]*\])?\s*$",
    )
    .captures_iter(output)
    {
        let status = match &c[1] {
            "Passed" => TestStatus::Passed,
            "Skipped" => TestStatus::Skipped,
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
    fn tap_from_node_test() {
        let out = "\
TAP version 13
# Subtest: cart
    # Subtest: adds items
    ok 1 - adds items
    not ok 2 - totals
    ok 3 - discounts # SKIP later
    1..3
not ok 1 - cart
1..1
";
        let r = parse(out);
        assert_eq!(r["cart > adds items"], TestStatus::Passed);
        assert_eq!(r["cart > totals"], TestStatus::Failed);
        assert_eq!(r["cart > discounts"], TestStatus::Skipped);
        assert!(!r.contains_key("cart"), "parents are not tests themselves");
    }

    #[test]
    fn spec_reporters() {
        let out = "PASS src/cart.test.js\n  cart\n    ✓ adds items (3 ms)\n    ✕ totals (1 ms)\n";
        let r = parse(out);
        assert_eq!(r["src/cart.test.js > adds items"], TestStatus::Passed);
        assert_eq!(r["src/cart.test.js > totals"], TestStatus::Failed);
        let r = parse("✔ adds items (0.5ms)\n✖ totals (1.2ms)\n");
        assert_eq!(r["adds items"], TestStatus::Passed);
        assert_eq!(r["totals"], TestStatus::Failed);
    }

    #[test]
    fn dotnet() {
        let r = parse("  Passed Shop.Tests.CartTests.Adds [2 ms]\n  Failed Shop.Tests.CartTests.Totals [1 ms]\n");
        assert_eq!(r["Shop.Tests.CartTests.Adds"], TestStatus::Passed);
        assert_eq!(r["Shop.Tests.CartTests.Totals"], TestStatus::Failed);
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
