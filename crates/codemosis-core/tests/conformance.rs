//! The language conformance suite: what every grammar in the deep tier must
//! do before a pack may rely on it. Each language has a small corpus in
//! `tests/corpus/<id>/`: `valid.*` (a comment containing MARK_COMMENT, a
//! string containing MARK_STRING and one definition) and `invalid.*` (one
//! syntax error).

use codemosis_core::lex::SegKind;
use codemosis_core::linguist::Confidence;
use codemosis_core::{lang, SourceFile};
use std::path::{Path, PathBuf};

fn corpus(id: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus").join(id)
}

fn file(id: &str, stem: &str) -> SourceFile {
    let dir = corpus(id);
    let path = std::fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("{id} has no corpus in {}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_stem()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(stem))
        })
        .unwrap_or_else(|| panic!("{id}: no {stem}.* in the corpus"));
    let text = std::fs::read_to_string(&path).unwrap();
    SourceFile::new(path.file_name().unwrap().to_string_lossy().to_string(), text)
}

#[test]
fn every_grammar_conforms() {
    let mut failures = Vec::new();
    for l in lang::all() {
        let id = l.id();
        let mut fail = |what: String| failures.push(format!("{id}: {what}"));

        let valid = file(id, "valid");
        match valid.lang {
            Some(g) if g.id() == id => {}
            other => fail(format!("valid file detected as {:?}", other.map(|g| g.id()))),
        }
        if valid.detection.confidence == Confidence::Guess {
            fail("the corpus file's language is only a guess".into());
        }
        if valid.syntax_errors() != 0 {
            fail(format!("valid file has {} syntax error(s)", valid.syntax_errors()));
        }
        let at = |mark: &str| valid.text.find(mark).map(|i| valid.kind_at(i));
        if at("MARK_COMMENT") != Some(Some(SegKind::Comment)) {
            fail(format!(
                "MARK_COMMENT is read as {:?}, not a comment",
                at("MARK_COMMENT")
            ));
        }
        if at("MARK_STRING") != Some(Some(SegKind::String)) {
            fail(format!("MARK_STRING is read as {:?}, not a string", at("MARK_STRING")));
        }
        let defined = valid.root().is_some_and(|root| {
            let mut stack = vec![root];
            let mut found = false;
            while let Some(n) = stack.pop() {
                if l.definition_name(&n, &valid.text).is_some() {
                    found = true;
                    break;
                }
                let mut c = n.walk();
                stack.extend(n.children(&mut c));
            }
            found
        });
        if !defined {
            fail("no definition is recognized in the valid file (definition kinds)".into());
        }

        let invalid = file(id, "invalid");
        if invalid.syntax_errors() == 0 {
            fail("the invalid file parses without errors".into());
        }
        if invalid.parse_damage().is_empty() {
            fail("the invalid file reports no damaged region".into());
        }

        // Pack examples force their language, so an extension shared with
        // another language is fine, as long as the code decides it.
        let ex = SourceFile::new(l.example_file(), valid.text.clone());
        if ex.lang.map(|g| g.id()) != Some(id) {
            fail(format!(
                "the example file name {} is not detected as {id}, even holding {id} code",
                l.example_file()
            ));
        }
        if lang::grammar_crate(id).is_empty() {
            fail("no grammar crate is recorded".into());
        }
    }
    assert!(
        failures.is_empty(),
        "conformance failures:\n  {}",
        failures.join("\n  ")
    );
}
