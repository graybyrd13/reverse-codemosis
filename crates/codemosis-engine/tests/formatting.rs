//! The formatting contract, end to end: a file the project's formatter
//! leaves unchanged must stay that way after a correction, reformatting may
//! only touch lines next to the correction, and a certificate records the
//! layout so `proof verify` can replay it.
//!
//! These use gofmt, which needs no configuration; they are skipped where
//! Go is not installed.

use codemosis_core::pack::{self, PackSource};
use codemosis_core::{FileFilter, Pack, Project};
use codemosis_engine::answers::Answers;
use codemosis_engine::formatter::{FormatSection, Formatters, Kind, Mode};
use codemosis_engine::proof::{verify, Certificate};
use codemosis_engine::session::{Options, Report, Session};
use std::path::Path;

fn gofmt_installed() -> bool {
    let ok = std::process::Command::new("gofmt")
        .arg("-l")
        .arg("/dev/null")
        .output()
        .is_ok();
    if !ok {
        eprintln!("gofmt is not installed; skipped");
    }
    ok
}

fn go_pack(rule: &str) -> Pack {
    let src = PackSource {
        origin: "formatting".into(),
        manifest:
            "[pack]\nid = \"layout\"\nversion = \"0.1.0\"\nlanguage = \"go\"\nsummary = \"x\"\nlicense = \"MIT\"\n"
                .into(),
        rules: vec![("rules/r.toml".into(), rule.to_string())],
        tables: vec![],
    };
    pack::compile(&src).unwrap_or_else(|e| panic!("{e}"))
}

/// `x == true` becomes `x` followed by a stray space: right, but unformatted.
const SLOPPY: &str = r#"
id = "r"
title = "Drop == true, leaving a space behind"
category = "simplification"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(binary_expression left: (_) @l operator: \"==\" right: (true)) @e"
[fix]
edits = [{ replace = "@e", with = "${l} " }]
[[example]]
before = "package p\n\nfunc f(x bool) bool {\n\tif x == true {\n\t\treturn true\n\t}\n\treturn false\n}\n"
after = "package p\n\nfunc f(x bool) bool {\n\tif x  {\n\t\treturn true\n\t}\n\treturn false\n}\n"
run = false
[[counterexample]]
code = "package p\n"
"#;

const CODE: &str = "package p\n\nfunc f(x bool) bool {\n\tif x == true {\n\t\treturn true\n\t}\n\treturn false\n}\n";

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let p = dir.path().join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir
}

fn run(root: &Path, pack: &Pack, mode: Mode, write: bool) -> (Report, Project) {
    let mut project = Project::load(root, &FileFilter::default()).unwrap();
    let formatters = Formatters::new(
        root,
        &FormatSection {
            mode,
            ..FormatSection::default()
        },
        false,
    );
    let none = |_: &str| None;
    let report = Session {
        project: &mut project,
        rules: pack.rules.iter().collect(),
        answers: &none,
        oracle: None,
        policy: None,
        formatters: Some(&formatters),
        options: Options { write, max_steps: 20 },
    }
    .run(&mut |_| {})
    .unwrap();
    (report, project)
}

fn reasons(r: &Report) -> String {
    r.rejected
        .iter()
        .flat_map(|x| x.reasons.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_correction_that_unformats_a_formatted_file_is_refused() {
    if !gofmt_installed() {
        return;
    }
    let dir = project(&[("main.go", CODE)]);
    let (r, p) = run(dir.path(), &go_pack(SLOPPY), Mode::Refuse, false);
    assert!(r.steps.is_empty(), "{:#?}", r.steps);
    assert!(
        reasons(&r).contains("main.go is kept formatted with gofmt") && reasons(&r).contains("line 4"),
        "{}",
        reasons(&r)
    );
    assert_eq!(p.files[0].text, CODE);
    assert_eq!(r.formatting[0].files_checked, 1);
}

#[test]
fn a_file_that_was_not_formatted_is_not_held_to_it() {
    if !gofmt_installed() {
        return;
    }
    let messy = CODE.replace("return false", "return   false");
    let dir = project(&[("main.go", &messy)]);
    let (r, _) = run(dir.path(), &go_pack(SLOPPY), Mode::Refuse, false);
    assert_eq!(r.steps.len(), 1, "{}", reasons(&r));
    assert!(r.steps[0].formatted.is_empty());
    assert_eq!(r.formatting[0].files_not_formatted, 1);
}

#[test]
fn reformat_lays_out_the_corrected_lines() {
    if !gofmt_installed() {
        return;
    }
    let dir = project(&[("main.go", CODE)]);
    let (r, p) = run(dir.path(), &go_pack(SLOPPY), Mode::Reformat, false);
    assert_eq!(r.steps.len(), 1, "{}", reasons(&r));
    assert_eq!(p.files[0].text, CODE.replace("x == true", "x"));
    assert_eq!(r.steps[0].formatted[0].formatter, "gofmt");
}

#[test]
fn reformatting_away_from_the_correction_is_refused() {
    if !gofmt_installed() {
        return;
    }
    // gofmt aligns a whole const block: shortening the longest name moves
    // every `=`, including lines far from the correction.
    let code = "package p\n\nconst (\n\tlongest = 1\n\tb       = 2\n\tc       = 3\n\td       = 4\n\te       = 5\n\tf       = 6\n\tg       = 7\n\th       = 8\n)\n";
    let rule = r#"
id = "r"
title = "Rename (on purpose)"
category = "simplification"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(const_spec name: (identifier) @n (#eq? @n \"longest\"))"
[fix]
edits = [{ replace = "@n", with = "a" }]
[[example]]
before = "package p\n\nconst longest = 1\n"
after = "package p\n\nconst a = 1\n"
run = false
[[counterexample]]
code = "package p\n"
"#;
    let dir = project(&[("consts.go", code)]);
    let (r, p) = run(dir.path(), &go_pack(rule), Mode::Reformat, false);
    assert!(r.steps.is_empty(), "{:#?}", r.steps);
    assert!(reasons(&r).contains("away from the correction"), "{}", reasons(&r));
    assert_eq!(p.files[0].text, code);
}

#[test]
fn certificates_replay_the_layout() {
    if !gofmt_installed() {
        return;
    }
    // gofmt sorts imports, so its layout here changes more than whitespace:
    // only gofmt itself can replay it.
    let code = "package p\n\nimport (\n\t\"bufio\"\n\t\"fmt\"\n)\n\nvar _ = fmt.Sprint\nvar _ = bufio.NewReader\n";
    let rule = r#"
id = "r"
title = "Swap an import (on purpose)"
category = "simplification"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(import_spec path: (interpreted_string_literal) @p (#eq? @p \"\\\"bufio\\\"\"))"
[fix]
edits = [{ replace = "@p", with = "\"os\"" }]
[[example]]
before = "package p\n\nimport \"bufio\"\n"
after = "package p\n\nimport \"os\"\n"
run = false
[[counterexample]]
code = "package p\n"
"#;
    let pack = go_pack(rule);
    let dir = project(&[("main.go", code)]);
    let root = dir.path();
    let (r, project) = run(root, &pack, Mode::Reformat, true);
    assert_eq!(r.steps.len(), 1, "{}", reasons(&r));
    let after = std::fs::read_to_string(root.join("main.go")).unwrap();
    assert!(
        after.contains("\"fmt\"\n\t\"os\""),
        "gofmt sorted the imports:\n{after}"
    );

    let cert = Certificate::of(
        &r,
        root,
        &project,
        std::slice::from_ref(&pack),
        &Answers::default(),
        None,
    )
    .unwrap();
    assert_eq!(cert.steps[0].formatted[0].formatter, Kind::Gofmt.name());

    // With gofmt available, the layout is replayed exactly.
    let formatters = Formatters::new(root, &FormatSection::default(), false);
    let mut fresh = Project::load(root, &FileFilter::default()).unwrap();
    let v = verify(&cert, root, &mut fresh, std::slice::from_ref(&pack), Some(&formatters));
    assert!(v.ok(), "{:#?}", v.checks);
    assert!(v
        .checks
        .iter()
        .any(|(name, _)| name.contains("laid out main.go identically")));

    // Without it, a layout that reorders tokens cannot be taken on trust.
    let mut fresh = Project::load(root, &FileFilter::default()).unwrap();
    let v = verify(&cert, root, &mut fresh, std::slice::from_ref(&pack), None);
    assert!(!v.ok());
    let why = format!("{:?}", v.checks.last().unwrap().1);
    assert!(why.contains("can be replayed only with gofmt"), "{why}");
}

#[test]
fn whitespace_only_layout_verifies_without_the_formatter() {
    if !gofmt_installed() {
        return;
    }
    let pack = go_pack(SLOPPY);
    let dir = project(&[("main.go", CODE)]);
    let root = dir.path();
    let (r, project) = run(root, &pack, Mode::Reformat, true);
    assert_eq!(r.steps.len(), 1, "{}", reasons(&r));
    let cert = Certificate::of(
        &r,
        root,
        &project,
        std::slice::from_ref(&pack),
        &Answers::default(),
        None,
    )
    .unwrap();
    let mut fresh = Project::load(root, &FileFilter::default()).unwrap();
    let v = verify(&cert, root, &mut fresh, std::slice::from_ref(&pack), None);
    assert!(v.ok(), "{:#?}", v.checks);
    assert!(v
        .checks
        .iter()
        .any(|(name, _)| name.contains("changes whitespace only")));
}
