//! The proof: one broken repository, one command, a green test suite, and a
//! diff a careful human can read line by line (checked in as
//! `examples/broken-inventory.diff`).
//!
//! Regenerate the golden diff after an intended change with
//! `UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo`.

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn codemosis(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(args)
        .arg(dir)
        .args(["--color", "never"])
        .output()
        .expect("run codemosis")
}

fn have(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap().flatten() {
        let p = entry.path();
        let name = entry.file_name();
        if name == "__pycache__" {
            continue;
        }
        if p.is_dir() {
            copy_dir(&p, &dst.join(&name));
        } else {
            fs::copy(&p, dst.join(&name)).unwrap();
        }
    }
}

fn unittest_passes(dir: &Path) -> bool {
    Command::new("python3")
        .args(["-W", "ignore", "-m", "unittest", "discover", "-s", "tests", "-t", "."])
        .current_dir(dir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .expect("run python3")
        .status
        .success()
}

fn demo_copy() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let demo = tmp.path().join("broken-inventory");
    copy_dir(&workspace().join("examples/broken-inventory"), &demo);
    (tmp, demo)
}

fn fix_json(dir: &Path) -> Value {
    let out = codemosis(&["fix", "--allow-no-vcs", "--format", "json"], dir);
    assert!(
        out.status.success(),
        "codemosis fix failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("JSON report")
}

fn statuses(report: &Value, kind: &str) -> Vec<Value> {
    report["remaining"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["status"]["kind"] == kind)
        .cloned()
        .collect()
}

#[test]
fn one_command_turns_the_broken_demo_green() {
    if !have("python3") {
        eprintln!("skipping: python3 is not available");
        return;
    }
    let (_tmp, demo) = demo_copy();
    assert!(!unittest_passes(&demo), "the demo must start with a failing test suite");

    let report = fix_json(&demo);
    assert!(
        unittest_passes(&demo),
        "the test suite must be green after `codemosis fix`"
    );
    assert_eq!(report["result"]["build"]["ok"], true, "the build must pass");
    // (The baseline build fails on Python <= 3.13, where `except E, e:` does
    // not compile; from 3.14 it compiles and fails at run time instead.)
    assert!(
        report["rejected"].as_array().unwrap().is_empty(),
        "no correction should be rejected"
    );

    // Honest about the rest: three decisions for a human, one explanation.
    assert_eq!(statuses(&report, "needs_answer").len(), 3);
    assert_eq!(statuses(&report, "explained").len(), 1);
    assert_eq!(report["distance_after"]["broken"], 0);
    assert_eq!(report["distance_after"]["waste"], 0);

    let diff = report["diff"].as_str().unwrap();
    let golden = workspace().join("examples/broken-inventory.diff");
    if std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1") {
        fs::write(&golden, diff).unwrap();
    }
    let expected = fs::read_to_string(&golden).expect("examples/broken-inventory.diff");
    assert_eq!(
        diff, expected,
        "the diff changed; review it and run with UPDATE_GOLDEN=1 to accept"
    );

    // Running again finds nothing more to prove.
    let again = fix_json(&demo);
    assert!(again["steps"].as_array().unwrap().is_empty());
}

#[test]
fn answering_yes_applies_the_correction() {
    if !have("python3") {
        eprintln!("skipping: python3 is not available");
        return;
    }
    let (_tmp, demo) = demo_copy();
    let report = fix_json(&demo);
    let question = statuses(&report, "needs_answer")
        .into_iter()
        .find(|f| f["rule"] == "python-core/mutable-default")
        .expect("the mutable default is a question");
    let id = question["status"]["question"]["id"].as_str().unwrap().to_string();

    let out = Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(["answer", &id, "yes"])
        .arg(&demo)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let ledger = fs::read_to_string(demo.join(".codemosis/answers.toml")).unwrap();
    assert!(ledger.contains(&id) && ledger.contains("answer = \"yes\""));

    let second = fix_json(&demo);
    let diff = second["diff"].as_str().unwrap();
    assert!(diff.contains("-    def restock(self, sku: str, amount: int, history=[]):"));
    assert!(diff.contains("+        if history is None:\n+            history = []"));
    assert!(unittest_passes(&demo));
    assert_eq!(statuses(&second, "needs_answer").len(), 2);
}

#[test]
fn a_correction_that_breaks_a_test_is_rolled_back() {
    if !have("python3") {
        eprintln!("skipping: python3 is not available");
        return;
    }
    // Removing a redundant `pass` preserves behaviour under the rule's
    // semantic model, but this test reads the source text, which the model
    // does not cover. The oracle catches it and the step is undone.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::create_dir_all(dir.join("tests")).unwrap();
    fs::write(
        dir.join("marker.py"),
        "class Marker:\n    \"\"\"Docs.\"\"\"\n    pass\n",
    )
    .unwrap();
    fs::write(dir.join("tests/__init__.py"), "").unwrap();
    fs::write(
        dir.join("tests/test_marker.py"),
        "import pathlib\nimport unittest\n\n\nclass T(unittest.TestCase):\n    def test_source_has_pass(self):\n        self.assertIn(\"pass\", pathlib.Path(\"marker.py\").read_text())\n",
    )
    .unwrap();
    fs::write(
        dir.join("codemosis.toml"),
        "[oracle]\ntest = \"python3 -m unittest discover -s tests -t . -v\"\n",
    )
    .unwrap();

    let report = fix_json(dir);
    let rejected = report["rejected"].as_array().unwrap();
    assert_eq!(rejected.len(), 1);
    assert_eq!(rejected[0]["finding"]["rule"], "python-core/redundant-pass");
    let reasons = rejected[0]["reasons"].to_string();
    assert!(reasons.contains("test_source_has_pass now fails"), "{reasons}");
    assert!(fs::read_to_string(dir.join("marker.py")).unwrap().contains("pass"));
    assert!(report["steps"].as_array().unwrap().is_empty());
}

#[test]
fn fix_refuses_a_dirty_working_tree() {
    if !have("git") {
        eprintln!("skipping: git is not available");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    fs::write(dir.join("a.py"), "import os\n").unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "init"]);
    fs::write(dir.join("a.py"), "import os\nimport sys\n").unwrap();
    let out = codemosis(&["fix"], dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("uncommitted changes"));
    assert_eq!(fs::read_to_string(dir.join("a.py")).unwrap(), "import os\nimport sys\n");
}

#[test]
fn builtin_packs_pass_their_self_tests() {
    let out = Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(["pack", "test", "--color", "never"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("0 failed"), "{text}");
}

#[test]
fn facts_reference_is_current() {
    let out = Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(["facts", "--color", "never"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let expected = format!(
        "# Fact vocabulary\n\n\
         The functions, template filters and edit operations that rules can use.\n\
         Every fact is conservative: when it cannot be sure, it answers in the\n\
         direction that blocks a correction. See [PACK_FORMAT.md](PACK_FORMAT.md).\n\n\
         This file is generated from `codemosis facts`; regenerate it with\n\
         `UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo`.\n\n\
         ```text\n{}```\n",
        String::from_utf8_lossy(&out.stdout)
    );
    let path = workspace().join("docs/FACTS.md");
    if std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1") {
        fs::write(&path, &expected).unwrap();
    }
    assert_eq!(
        fs::read_to_string(&path).unwrap_or_default(),
        expected,
        "docs/FACTS.md is stale"
    );
}

fn run_with_input(args: &[&str], dir: &Path, input: &str) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(args)
        .arg(dir)
        .args(["--color", "never"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run codemosis");
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn node_tests_pass(dir: &Path) -> bool {
    Command::new("node")
        .arg("--test")
        .current_dir(dir)
        .output()
        .expect("run node")
        .status
        .success()
}

#[test]
fn interactive_answers_drive_the_javascript_demo() {
    if !have("node") || !have("npm") {
        eprintln!("skipping: node and npm are not available");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let demo = tmp.path().join("flaky-checkout");
    copy_dir(&workspace().join("examples/flaky-checkout"), &demo);
    assert!(!node_tests_pass(&demo), "the demo must start with a failing test");

    // Three questions: two fixed waits, one console.log. An answer that is
    // not one of the choices is asked again.
    let out = run_with_input(
        &["fix", "--interactive", "--allow-no-vcs", "--format", "json"],
        &demo,
        "remove\nremove\nmaybe\nremove\n",
    );
    let prompts = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{prompts}");
    assert!(prompts.contains("Keep this fixed wait?"), "{prompts}");
    assert!(prompts.contains("please answer keep/remove"), "{prompts}");
    let report: Value = serde_json::from_slice(&out.stdout).expect("JSON report");

    assert!(node_tests_pass(&demo), "the suite must be green after the corrections");
    assert!(report["rejected"].as_array().unwrap().is_empty());
    assert_eq!(
        statuses(&report, "needs_answer").len(),
        0,
        "every question was answered"
    );
    assert_eq!(
        statuses(&report, "explained").len(),
        1,
        "the duplicate key is explained"
    );
    let ledger = fs::read_to_string(demo.join(".codemosis/answers.toml")).unwrap();
    assert_eq!(ledger.matches("answer = \"remove\"").count(), 3);

    let diff = report["diff"].as_str().unwrap();
    let golden = workspace().join("examples/flaky-checkout.diff");
    if std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "1") {
        fs::write(&golden, diff).unwrap();
    }
    let expected = fs::read_to_string(&golden).expect("examples/flaky-checkout.diff");
    assert_eq!(
        diff, expected,
        "the diff changed; review it and run with UPDATE_GOLDEN=1 to accept"
    );
}

#[test]
fn keep_is_recorded_and_respected() {
    if !have("node") {
        eprintln!("skipping: node is not available");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let demo = tmp.path().join("flaky-checkout");
    copy_dir(&workspace().join("examples/flaky-checkout"), &demo);
    let out = run_with_input(&["ask"], &demo, "keep\nskip\nquit\n");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("1 answer recorded"), "{text}");

    // The kept wait is no longer a question, and stays in the code.
    let check = codemosis(&["check", "--format", "json"], &demo);
    let report: Value = serde_json::from_slice(&check.stdout).unwrap();
    let declined: Vec<_> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["status"]["kind"] == "declined")
        .collect();
    assert_eq!(declined.len(), 1);
    assert_eq!(declined[0]["status"]["answer"], "keep");
    assert!(fs::read_to_string(demo.join("src/checkout.js"))
        .unwrap()
        .contains("setTimeout(resolve, 300)"));

    // An answer that is not one of the choices is refused.
    let id = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["status"]["kind"] == "needs_answer")
        .unwrap()["status"]["question"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let bad = codemosis(&["answer", &id, "perhaps"], &demo);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("keep/remove"));
}

#[test]
fn every_github_language_is_known() {
    let out = Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(["languages", "--color", "never"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let summary = text.lines().last().unwrap();
    assert!(summary.starts_with("845 languages known to GitHub"), "{summary}");
}
