//! What agentic coding tools use, end to end through the real binary:
//! `check --file/--changed --format compact` and `codemosis hook`.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    assert!(ok, "git {args:?}");
}

/// A committed file with an old issue, then an edit that adds two new ones.
fn repo() -> Option<tempfile::TempDir> {
    Command::new("git").arg("--version").output().ok()?;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("app")).unwrap();
    let old = "import time\n\n\ndef old(x):\n    if x is 5:\n        print(x)\n";
    std::fs::write(root.join("app/core.py"), old).unwrap();
    std::fs::write(root.join("app/other.py"), "def other(y):\n    return y is 7\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);
    let new = format!("{old}\n\ndef new(items):\n    if len(items) is 0:\n        return None\n    time.sleep(2)\n    return items[0]\n");
    std::fs::write(root.join("app/core.py"), new).unwrap();
    Some(dir)
}

fn run(dir: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run codemosis");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn the_hook_reports_only_what_the_agent_changed() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let payload = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "cwd": root,
        "tool_input": {"file_path": root.join("app/core.py")},
    });
    let out = run(root, &["hook"], &payload.to_string());
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{err}");
    assert!(err.contains("found 2 issues in the code you just changed"), "{err}");
    assert!(
        err.contains("app/core.py:10:8 python-core/is-literal (proven correction"),
        "{err}"
    );
    assert!(
        err.contains("+    if len(items) == 0:"),
        "the correction is shown: {err}"
    );
    assert!(
        err.contains("Ask the user: Keep this fixed wait?") && err.contains("Do not choose for them"),
        "{err}"
    );
    assert!(
        !err.contains(":5:"),
        "the old issue on an unchanged line is not reported: {err}"
    );
    assert!(
        !err.contains("other.py"),
        "files the agent did not touch are not reported: {err}"
    );

    // Every finding in the file, not only changed lines.
    let out = run(root, &["hook", "--all-lines"], &payload.to_string());
    assert!(String::from_utf8_lossy(&out.stderr).contains("app/core.py:5:8"));

    // A stop hook names no file: every changed file is checked.
    let out = run(
        root,
        &["hook"],
        r#"{"hook_event_name": "Stop", "stop_hook_active": false}"#,
    );
    assert_eq!(out.status.code(), Some(2));
    // ...but never twice in a row, which could loop on a question only the user can answer.
    let out = run(
        root,
        &["hook"],
        r#"{"hook_event_name": "Stop", "stop_hook_active": true}"#,
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty());

    // JSON for tools that want structure.
    let out = run(root, &["hook", "--json"], &payload.to_string());
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["schema"], "codemosis/hook/v1");
    assert_eq!(v["findings"].as_array().unwrap().len(), 2);
}

#[test]
fn nothing_to_report_is_silent() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    git(root, &["checkout", "-q", "--", "."]);
    let out = run(root, &["hook"], "{}");
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty() && out.stdout.is_empty());
}

#[test]
fn check_scoped_to_files_and_changes() {
    let Some(dir) = repo() else { return };
    let root = dir.path();
    let out = run(
        root,
        &[
            "check",
            "--file",
            "app/other.py",
            "--format",
            "compact",
            "--fail-on",
            "never",
        ],
        "",
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(
        text.starts_with("app/other.py:2:12: error: ") && text.contains("[python-core/is-literal; proven correction]"),
        "{text}"
    );

    let out = run(
        root,
        &["check", "--changed", "--format", "compact", "--fail-on", "never"],
        "",
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.lines().all(|l| l.starts_with("app/core.py:")), "{text}");
    assert!(text.contains("question Q-"), "{text}");
}
