//! Security regression tests: each one pins a defence described in
//! docs/THREAT_MODEL.md, so removing the defence fails the build.

use codemosis_core::pack::{self, PackSource};
use codemosis_core::redact::redact;
use codemosis_core::{scan, Project, SourceFile};

fn pack_with_example_path(path: &str) -> PackSource {
    PackSource {
        origin: "test".into(),
        manifest: r#"
[pack]
id = "evil"
version = "0.1.0"
language = "python"
summary = "x"
license = "MIT"
authors = ["x"]
"#
        .into(),
        rules: vec![(
            "r.toml".into(),
            format!(
                r#"
id = "r"
title = "r"
category = "waste"
summary = "s"
explain = "e"
shape = "(pass_statement) @p"

[[example]]
path = "{path}"
before = "pass\n"

[[counterexample]]
code = "x = 1\n"
"#
            ),
        )],
        tables: vec![],
    }
}

#[test]
fn pack_example_paths_cannot_escape() {
    for bad in ["../../etc/cron.d/x.py", "/tmp/x.py", "a/../../x.py", "C:\\\\x.py"] {
        let err = pack::compile(&pack_with_example_path(bad)).err();
        assert!(
            err.as_ref().is_some_and(|e| e.to_string().contains("relative path")),
            "{bad} was accepted: {err:?}"
        );
    }
    assert!(pack::compile(&pack_with_example_path("pkg/mod.py")).is_ok());
}

#[test]
fn findings_never_print_a_secret() {
    let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQ\n-----END OPENSSH PRIVATE KEY-----\n";
    let project = Project::in_memory(vec![SourceFile::new("deploy/key.txt", key.to_string())]);
    let packs = pack::builtin().unwrap();
    let rules: Vec<_> = packs.iter().flat_map(|p| p.rules.iter()).collect();
    let none = |_: &str| None;
    let s = scan(&project, rules, &none, None);
    let f = s
        .findings
        .iter()
        .find(|f| f.rule == "universal/private-key")
        .expect("the key is found");
    let json = format!("{f:?}");
    assert!(!json.contains("b3BlbnNzaC1rZXkt"), "{json}");
    assert!(!redact(key).contains("b3BlbnNzaC1rZXkt"));
}
