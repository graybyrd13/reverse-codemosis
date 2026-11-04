//! Property tests: invariants that must hold for any input, checked over
//! thousands of generated cases. The generator is a fixed-seed xorshift, so
//! a failure reproduces exactly; raise CODEMOSIS_PROPTEST_CASES to search
//! harder.

use codemosis_core::lex::{self, SegKind};
use codemosis_core::model::Edit;
use codemosis_core::redact::redact;
use codemosis_core::{edit, expr, linguist, template, SourceFile};
use std::path::Path;

struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
    /// Text built from tokens that stress lexers: quotes, comment markers,
    /// escapes, newlines, multi-byte characters.
    fn code(&mut self, tokens: &[&str]) -> String {
        let n = self.below(40);
        (0..n).map(|_| self.pick(tokens)).collect()
    }
}

fn cases() -> usize {
    std::env::var("CODEMOSIS_PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000)
}

const TOKENS: &[&str] = &[
    "\"", "'", "`", "\"\"\"", "'''", "#", "//", "/*", "*/", "--", "<!--", "-->", ";", "%", "\\", "\n", " ", "x", "é",
    "😀", "{", "}", "(", ")", "$", "@", "r\"", "[[", "]]", "=begin", "=end", "\t", "0", "\r\n",
];

#[test]
fn lexical_segments_tile_the_text() {
    let mut g = Gen(0x1234_5678_9abc_def1);
    let langs: Vec<_> = linguist::all()
        .iter()
        .filter(|l| lex::profile_for(l).is_some())
        .collect();
    for _ in 0..cases() {
        let lang = langs[g.below(langs.len())];
        let profile = lex::profile_for(lang).unwrap();
        let text = g.code(TOKENS);
        let segs = lex::segments(&text, profile);
        let mut at = 0;
        for s in &segs {
            assert!(
                s.start >= at && s.start <= s.end && s.end <= text.len(),
                "{}: {segs:?} in {text:?}",
                lang.name
            );
            assert!(text.is_char_boundary(s.start) && text.is_char_boundary(s.end));
            at = s.end;
        }
        for i in 0..=text.len() {
            let _ = lex::kind_at(&segs, i);
        }
    }
}

#[test]
fn source_files_never_panic_and_agree_with_their_segments() {
    let mut g = Gen(0xfeed_beef_0bad_cafe);
    let names = [
        "a.py",
        "a.rs",
        "a.js",
        "a.go",
        "a.c",
        "a.rb",
        "a.sh",
        "a.ps1",
        "a.sql",
        "Makefile",
        "a.h",
        "a.unknownext",
    ];
    for _ in 0..cases() / 4 {
        let name = g.pick(&names);
        let text = g.code(TOKENS);
        let f = SourceFile::new(name, text.clone());
        let _ = f.syntax_errors();
        let _ = f.parse_damage();
        if f.knows_comments() {
            for i in (0..text.len()).filter(|i| text.is_char_boundary(*i)) {
                assert!(f.kind_at(i).is_some());
            }
        }
        let _ = codemosis_core::suppress::markers(&f);
        let _ = codemosis_core::guards::conditional_regions(&f);
    }
}

#[test]
fn the_fact_and_template_parsers_never_panic() {
    let mut g = Gen(0x0dd_ba11_5eed);
    let pieces = [
        "@a", "@", "text(", ")", "(", "'", "\"", "&&", "||", "!", "==", "!=", "in", "not", "[", "]", ",", " ", "1",
        "-", "py.bound", ".", "${", "}", "|", ":", "$$", "$", "trim", "é",
    ];
    for _ in 0..cases() {
        let src = g.code(&pieces);
        let _ = expr::parse(&src);
        let _ = template::Template::parse(&src);
    }
}

#[test]
fn redaction_keeps_shape_and_is_idempotent() {
    let mut g = Gen(0xabad_1dea);
    let pieces = [
        "ghp_",
        "AKIA",
        "abcdefghijklmnopqrstuvwxyz0123456789",
        "password",
        " = ",
        "\"",
        "'",
        "-----BEGIN RSA PRIVATE KEY-----",
        "-----END RSA PRIVATE KEY-----",
        "\n",
        "+",
        "MIIEow",
        "postgres://u:",
        "@h/",
        "sk-",
        "xoxb-",
        "é",
        " ",
    ];
    for _ in 0..cases() {
        let text = g.code(&pieces);
        let once = redact(&text).into_owned();
        assert_eq!(
            once.chars().count(),
            text.chars().count(),
            "redaction changed the length of {text:?}"
        );
        assert_eq!(once.lines().count(), text.lines().count());
        assert_eq!(redact(&once), once, "redaction is not idempotent on {text:?}");
    }
}

#[test]
fn applying_edits_matches_their_arithmetic() {
    let mut g = Gen(0x5ca1_ab1e);
    for _ in 0..cases() {
        let text: String = g.code(&["a", "b", "\n", "é", " "]);
        let bounds: Vec<usize> = (0..=text.len()).filter(|i| text.is_char_boundary(*i)).collect();
        let mut points: Vec<usize> = (0..g.below(6) * 2).map(|_| bounds[g.below(bounds.len())]).collect();
        points.sort_unstable();
        let edits: Vec<Edit> = points
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| Edit {
                start: c[0],
                end: c[1],
                text: "Z".repeat(g.below(3)),
            })
            .collect();
        match edit::apply(&text, &edits) {
            Ok(out) => {
                // Identical edits (two findings proposing the same change) apply once.
                let applied = edit::normalize(&edits).expect("apply succeeded");
                let delta: isize = applied
                    .iter()
                    .map(|e| e.text.len() as isize - (e.end - e.start) as isize)
                    .sum();
                assert_eq!(out.len() as isize, text.len() as isize + delta);
            }
            Err(_) => assert!(
                edit::normalize(&edits).is_err(),
                "apply failed on valid edits {edits:?}"
            ),
        }
    }
}

#[test]
fn detection_never_panics_and_respects_confidence() {
    let mut g = Gen(0xc0ff_ee00);
    let stems = ["main", "Makefile", "Dockerfile", ".bashrc", "x", "README", "CMakeLists"];
    let exts = [
        "", ".h", ".m", ".pl", ".ts", ".php", ".py", ".rs", ".yml", ".json", ".inc", ".v", ".zz",
    ];
    let heads = [
        "",
        "#!/usr/bin/env python3\n",
        "#!/bin/sh\n",
        "<?php\n",
        "<?xml version=\"1.0\"?>\n",
        "#include <stdio.h>\n",
        "@interface X\n",
    ];
    for _ in 0..cases() {
        let name = format!("{}{}", g.pick(&stems), g.pick(&exts));
        let text = format!("{}{}", g.pick(&heads), g.code(TOKENS));
        let d = linguist::detect(Path::new(&name), &text);
        let _ = linguist::role(Path::new(&name), &text);
        if d.language.is_none() {
            assert_ne!(
                d.confidence,
                linguist::Confidence::Certain,
                "{name}: no language, yet certain"
            );
        }
    }
}

#[test]
fn kinds_are_one_of_three() {
    let f = SourceFile::new("a.py", "x = 'y'  # z\n".to_string());
    let kinds: Vec<SegKind> = (0..f.text.len()).filter_map(|i| f.kind_at(i)).collect();
    assert!(kinds.contains(&SegKind::Code) && kinds.contains(&SegKind::String) && kinds.contains(&SegKind::Comment));
}
