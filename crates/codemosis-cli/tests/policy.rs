//! Policy end to end: inline and configured suppressions (with expiry),
//! baselines, severity levels and the CI gate, through the real binary.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(args)
        .args(["--color", "never"])
        .current_dir(dir)
        .env("CODEMOSIS_TODAY", "2026-11-04")
        .env_remove("GITHUB_STEP_SUMMARY")
        .output()
        .expect("run codemosis")
}

fn json(dir: &Path, extra: &[&str]) -> Value {
    let mut args = vec!["check", ".", "--format", "json"];
    args.extend_from_slice(extra);
    let out = run(dir, &args);
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)))
}

fn statuses(v: &Value) -> Vec<(String, String)> {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["rule"].as_str().unwrap().to_string(),
                f["status"]["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

const CODE: &str = "\
def load(path):
    try:
        return open(path).read()
    except:  # codemosis: ignore[python-core/bare-except] the caller logs and retries
        return None


def save(path, data):
    try:
        open(path, 'w').write(data)
    except:
        pass


def old(path):
    try:
        return path.upper()
    # codemosis: ignore[python-core] until=2026-01-31 legacy
    except:
        return None
";

#[test]
fn inline_suppressions_apply_until_they_expire() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("store.py"), CODE).unwrap();
    let v = json(dir.path(), &[]);
    let st = statuses(&v);
    let bare: Vec<&(String, String)> = st.iter().filter(|(r, _)| r == "python-core/bare-except").collect();
    assert_eq!(bare.len(), 3, "{st:?}");
    assert_eq!(bare[0].1, "suppressed", "line 4 has a marker on its own line");
    assert_ne!(bare[1].1, "suppressed", "line 11 has no marker");
    assert_ne!(bare[2].1, "suppressed", "the marker above line 19 expired");
    let expired = &v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "python-core/bare-except")
        .nth(2)
        .unwrap()["evidence"];
    assert!(expired.to_string().contains("expired on 2026-01-31"), "{expired}");
    let first = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["rule"] == "python-core/bare-except")
        .unwrap();
    assert_eq!(first["status"]["suppression"]["reason"], "the caller logs and retries");
    assert_eq!(first["status"]["suppression"]["kind"], "in-source");
}

#[test]
fn configured_suppressions_severity_and_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("legacy")).unwrap();
    fs::write(
        dir.path().join("legacy/store.py"),
        CODE.replace("# codemosis", "# note"),
    )
    .unwrap();
    fs::write(
        dir.path().join("codemosis.toml"),
        r#"
[policy]
fail_on = "error"

[severity]
"python-core/bare-except" = "warning"

[[suppress]]
rule = "python-core/bare-except"
paths = ["legacy/**"]
reason = "legacy module, replaced in Q1"
owner = "@storage"
"#,
    )
    .unwrap();
    let v = json(dir.path(), &[]);
    let st = statuses(&v);
    assert!(
        st.iter()
            .filter(|(r, _)| r == "python-core/bare-except")
            .all(|(_, s)| s == "suppressed"),
        "{st:?}"
    );
    let out = run(dir.path(), &["check", "."]);
    assert!(out.status.success(), "fail_on = error and nothing open is an error");
    let out = run(dir.path(), &["check", ".", "--fail-on", "note"]);
    let any_open = st.iter().any(|(_, s)| s != "suppressed" && s != "declined");
    assert_eq!(!out.status.success(), any_open);

    // A suppression must say why.
    fs::write(
        dir.path().join("codemosis.toml"),
        "[[suppress]]\nrule = \"python-core\"\nreason = \"\"\n",
    )
    .unwrap();
    let out = run(dir.path(), &["check", "."]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("give a reason"));

    // And name a rule that exists.
    fs::write(
        dir.path().join("codemosis.toml"),
        "[severity]\n\"python-core/no-such-rule\" = \"error\"\n",
    )
    .unwrap();
    let out = run(dir.path(), &["check", "."]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a rule or pack id"));
}

#[test]
fn a_baseline_holds_the_line() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.py"), "def f(x):\n    return x is 5\n").unwrap();
    let out = run(dir.path(), &["baseline", "."]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let baseline = dir.path().join(".codemosis/baseline.json");
    let b: Value = serde_json::from_str(&fs::read_to_string(&baseline).unwrap()).unwrap();
    assert_eq!(b["schema"], "codemosis/baseline/v1");
    assert_eq!(b["findings"].as_array().unwrap().len(), 1);

    let out = run(dir.path(), &["check", ".", "--baseline", ".codemosis/baseline.json"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 accepted by the baseline"));

    // A new finding fails, even with the old one shifted down a line.
    fs::write(
        dir.path().join("a.py"),
        "import os\n\ndef f(x):\n    return x is 5\n\ndef g(y):\n    return y is 'a'\n",
    )
    .unwrap();
    let v = json(dir.path(), &["--baseline", ".codemosis/baseline.json"]);
    let baselined = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["baselined"] == true)
        .count();
    assert_eq!(baselined, 1, "the old finding keeps its fingerprint across line shifts");
    let out = run(dir.path(), &["check", ".", "--baseline", ".codemosis/baseline.json"]);
    assert_eq!(out.status.code(), Some(1));

    // SARIF marks the difference.
    let out = run(
        dir.path(),
        &[
            "check",
            ".",
            "--baseline",
            ".codemosis/baseline.json",
            "--format",
            "sarif",
        ],
    );
    let sarif: Value = serde_json::from_slice(&out.stdout).unwrap();
    let states: Vec<&str> = sarif["runs"][0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["ruleId"] == "python-core/is-literal")
        .map(|r| r["baselineState"].as_str().unwrap())
        .collect();
    assert_eq!(states.iter().filter(|s| **s == "unchanged").count(), 1, "{states:?}");
    assert_eq!(states.iter().filter(|s| **s == "new").count(), 1, "{states:?}");
}

#[test]
fn sarif_is_well_formed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.py"), "def f(x):\n    return x is 5\n").unwrap();
    let out = run(
        dir.path(),
        &["check", ".", "--format", "sarif", "--output", "out.sarif"],
    );
    assert!(out.status.code() == Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    let sarif: Value = serde_json::from_str(&fs::read_to_string(dir.path().join("out.sarif")).unwrap()).unwrap();
    assert_eq!(sarif["version"], "2.1.0");
    let run0 = &sarif["runs"][0];
    let rules = run0["tool"]["driver"]["rules"].as_array().unwrap();
    let r = &run0["results"][0];
    let idx = r["ruleIndex"].as_u64().unwrap() as usize;
    assert_eq!(rules[idx]["id"], r["ruleId"], "ruleIndex points at the rule");
    assert_eq!(r["level"], "error");
    assert!(r["partialFingerprints"]["codemosis/v1"].as_str().unwrap().len() == 32);
    assert_eq!(r["locations"][0]["physicalLocation"]["region"]["startLine"], 2);
    assert_eq!(
        r["fixes"][0]["artifactChanges"][0]["replacements"][0]["insertedContent"]["text"],
        "=="
    );
    let first_key = sarif.as_object().unwrap().keys().next().unwrap();
    assert_eq!(first_key, "$schema");
}
