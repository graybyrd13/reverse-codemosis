//! The soundness regression suite: rules and code built to trick the engine
//! into making code worse. Every case must be refused, and each names the
//! guard that refuses it. A change that lets any of these through is a
//! soundness bug, whatever else it improves.

use codemosis_core::pack::{self, PackSource};
use codemosis_core::{CompiledRule, Pack, Project, SourceFile, Status};
use codemosis_engine::session::{Options, Report, Session};

/// A one-rule pack, compiled.
fn pack(language: &str, rule: &str) -> Pack {
    let src = PackSource {
        origin: "soundness".into(),
        manifest: format!(
            "[pack]\nid = \"adversary\"\nversion = \"0.1.0\"\nlanguage = \"{language}\"\nsummary = \"x\"\nlicense = \"MIT\"\n"
        ),
        rules: vec![("rules/r.toml".into(), rule.to_string())],
        tables: vec![],
    };
    pack::compile(&src).unwrap_or_else(|e| panic!("{e}"))
}

fn session(files: Vec<SourceFile>, rules: Vec<&CompiledRule>) -> (Report, Project) {
    let mut project = Project::in_memory(files);
    let none = |_: &str| None;
    let report = Session {
        project: &mut project,
        rules,
        answers: &none,
        oracle: None,
        policy: None,
        options: Options {
            write: false,
            max_steps: 50,
        },
    }
    .run(&mut |_| {})
    .unwrap();
    (report, project)
}

fn reasons(r: &Report) -> String {
    let mut s: Vec<String> = r.rejected.iter().flat_map(|x| x.reasons.clone()).collect();
    for f in &r.remaining {
        if let Status::Refused { reason } = &f.status {
            s.push(reason.clone());
        }
    }
    s.join("\n")
}

fn builtin(id: &str) -> CompiledRule {
    let packs = pack::builtin().unwrap();
    let r = packs.iter().flat_map(|p| p.rules.iter()).find(|r| r.id == id).unwrap();
    CompiledRule::compile(&r.pack, r.spec.clone(), &r.languages, r.tables.clone()).unwrap()
}

const DELETE_ASSIGNMENTS: &str = r#"
id = "r"
title = "Delete constant assignments (unsound on purpose)"
category = "waste"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(expression_statement (assignment left: (identifier) @name right: (integer))) @stmt"
[fix]
edits = [{ delete_lines = "@stmt" }]
[[example]]
before = "x = 1\n"
after = ""
run = false
[[counterexample]]
code = "x = y\n"
"#;

#[test]
fn deleting_a_binding_that_is_read_elsewhere_is_refused() {
    // Guard: binding preservation. Deleting the local `x = 2` makes the
    // read of `x` resolve to the module's `x` instead.
    let p = pack("python", DELETE_ASSIGNMENTS);
    let code = "x = y\ndef f():\n    x = 2\n    return x\n";
    let (r, project) = session(
        vec![SourceFile::new("a.py", code.to_string())],
        p.rules.iter().collect(),
    );
    assert!(r.steps.is_empty(), "{:#?}", r.steps);
    assert!(reasons(&r).contains("changes what a name means"), "{}", reasons(&r));
    assert_eq!(project.files[0].text, code);
}

#[test]
fn shadowing_a_builtin_is_refused_unless_repairing() {
    let rule = r#"
id = "r"
title = "Insert a binding (unsound on purpose)"
category = "simplification"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(module . (expression_statement) @first)"
[fix]
edits = [{ insert_lines_before = "@first", with = "len = lambda x: 0" }]
[[example]]
before = "print(1)\n"
after = "len = lambda x: 0\nprint(1)\n"
run = false
[[counterexample]]
code = "\n"
"#;
    let p = pack("python", rule);
    let code = "print(len([1, 2]))\n";
    let (r, _) = session(
        vec![SourceFile::new("a.py", code.to_string())],
        p.rules.iter().collect(),
    );
    assert!(r.steps.is_empty());
    assert!(reasons(&r).contains("`len`"), "{}", reasons(&r));
}

#[test]
fn edits_inside_conditional_compilation_are_refused() {
    let rule = r#"
id = "r"
title = "Delete return 0 (on purpose)"
category = "waste"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(return_statement (number_literal) @n (#eq? @n \"0\")) @ret"
[fix]
edits = [{ delete_lines = "@ret" }]
[[example]]
before = "void f(void) {\n    return 0;\n}\n"
after = "void f(void) {\n}\n"
run = false
[[counterexample]]
code = "int f(void) { return 1; }\n"
"#;
    let p = pack("c", rule);
    let code = "int f(void) {\n#ifdef LEGACY\n    return 0;\n#endif\n    return 1;\n}\n";
    let (r, _) = session(vec![SourceFile::new("a.c", code.to_string())], p.rules.iter().collect());
    assert!(r.steps.is_empty());
    assert!(reasons(&r).contains("conditional-compilation"), "{}", reasons(&r));
}

#[test]
fn rust_items_behind_cfg_are_left_alone() {
    let rule = builtin("rust-core/unit-return-type");
    let code = "#[cfg(windows)]\nfn a() -> () {}\n\n#[cfg(test)]\nfn b() -> () {}\n";
    let (r, project) = session(vec![SourceFile::new("lib.rs", code.to_string())], vec![&rule]);
    assert_eq!(
        r.steps.len(),
        1,
        "only the cfg(test) item is corrected: {}",
        reasons(&r)
    );
    assert!(project.files[0].text.contains("fn a() -> () {}"));
    assert!(project.files[0].text.contains("fn b() {}"));
    assert!(reasons(&r).contains("cfg(windows)"));
}

#[test]
fn generated_and_vendored_files_are_never_edited() {
    let rule = builtin("python-core/is-literal");
    let code = "if x is 5:\n    pass\n";
    let generated = format!("# @generated by protoc. DO NOT EDIT.\n{code}");
    let (r, _) = session(
        vec![
            SourceFile::new("gen/api_pb2.py", generated),
            SourceFile::new("vendor/lib/x.py", code.to_string()),
            SourceFile::new("node_modules/m/x.py", code.to_string()),
        ],
        vec![&rule],
    );
    assert!(r.steps.is_empty() && r.remaining.is_empty(), "{:#?}", r.remaining);
}

#[test]
fn a_correction_never_deletes_a_comment() {
    let rule = r#"
id = "r"
title = "Replace function bodies (on purpose)"
category = "waste"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(function_definition body: (block) @body)"
[fix]
edits = [{ replace = "@body", with = "pass" }]
[[example]]
before = "def f():\n    x = 1\n"
after = "def f():\n    pass\n"
run = false
[[counterexample]]
code = "x = 1\n"
"#;
    let p = pack("python", rule);
    let code = "def f():\n    x = 1\n    # keep: the audit hook goes here\n    y = 2\n";
    let (r, project) = session(
        vec![SourceFile::new("a.py", code.to_string())],
        p.rules.iter().collect(),
    );
    assert!(r.steps.is_empty());
    assert!(reasons(&r).contains("delete the comment"), "{}", reasons(&r));
    assert_eq!(project.files[0].text, code);
}

#[test]
fn a_rule_that_matches_its_own_output_is_rejected() {
    let rule = r#"
id = "r"
title = "Never converges (on purpose)"
category = "simplification"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(integer) @n"
[fix]
edits = [{ replace = "@n", with = "(${n})" }]
[[example]]
before = "x = 1\n"
after = "x = (1)\n"
run = false
[[counterexample]]
code = "x = y\n"
"#;
    let p = pack("python", rule);
    let (r, _) = session(
        vec![SourceFile::new("a.py", "y = 1\n".into())],
        p.rules.iter().collect(),
    );
    assert!(r.steps.is_empty(), "{:#?}", r.steps);
    assert!(
        reasons(&r).contains("still matches after its own correction"),
        "{}",
        reasons(&r)
    );
}

#[test]
fn a_correction_that_breaks_the_syntax_is_rejected() {
    let rule = r#"
id = "r"
title = "Breaks syntax (on purpose)"
category = "simplification"
semantics = "preserved"
summary = "s"
explain = "e"
shape = "(return_statement) @ret"
[fix]
edits = [{ replace = "@ret", with = "return (" }]
[[example]]
before = "def f():\n    return 1\n"
after = "def f():\n    return (\n"
run = false
[[counterexample]]
code = "x = 1\n"
"#;
    let p = pack("python", rule);
    let code = "def f():\n    return 1\n";
    let (r, project) = session(vec![SourceFile::new("a.py", code.into())], p.rules.iter().collect());
    assert!(r.steps.is_empty());
    assert!(reasons(&r).contains("syntax error"), "{}", reasons(&r));
    assert_eq!(project.files[0].text, code);
}

#[test]
fn unparsed_code_is_not_corrected() {
    let rule = builtin("python-core/is-literal");
    let code = "def f(x:\n    if x is 5:\n        return 1\n";
    let (r, _) = session(vec![SourceFile::new("a.py", code.into())], vec![&rule]);
    assert!(r.steps.is_empty(), "{:#?}", r.steps);
    assert!(reasons(&r).contains("could not parse"), "{}", reasons(&r));
}

#[test]
fn a_language_detected_by_guesswork_gets_no_rules() {
    // `.h` with nothing that says C, C++ or Objective-C.
    let rule = builtin("c-family/assignment-in-condition");
    let (r, _) = session(vec![SourceFile::new("x.h", "\n".into())], vec![&rule]);
    assert!(r.remaining.is_empty());
}
