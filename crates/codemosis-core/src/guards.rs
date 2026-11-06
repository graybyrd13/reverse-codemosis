//! Places where a correction cannot be verified, whatever its rule says.
//!
//! The engine verifies every correction by building and testing the project,
//! but a build sees one configuration. Code inside `#if DEBUG ... #endif`,
//! under `#[cfg(target_os = "windows")]` or inside a macro's token trees may
//! not be compiled at all, so a correction there could never be checked.
//! Edits there are refused, with the reason.

use crate::lex::SegKind;
use crate::project::SourceFile;
use regex::Regex;
use std::sync::OnceLock;
use tree_sitter::Node;

/// Linguist languages with a C-style preprocessor (`#if ... #endif`).
const PREPROCESSED: &[&str] = &[
    "C",
    "C++",
    "C#",
    "Objective-C",
    "Objective-C++",
    "Swift",
    "Cuda",
    "HLSL",
    "GLSL",
];

fn directive() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^[ \t]*#[ \t]*(if|ifdef|ifndef|elif|elifdef|elifndef|else|endif)\b[ \t]*([A-Za-z_][A-Za-z0-9_]*)?",
        )
        .expect("valid regex")
    })
}

/// Byte ranges of conditionally compiled code: from each `#if`/`#ifdef`/
/// `#ifndef` to its `#endif`. An include guard (`#ifndef X` then
/// `#define X` around the whole file) is always compiled and is skipped.
pub fn conditional_regions(file: &SourceFile) -> Vec<(usize, usize)> {
    if !PREPROCESSED.contains(&file.language_name()) {
        return Vec::new();
    }
    let text = &file.text;
    let mut stack: Vec<(usize, bool)> = Vec::new();
    let mut out = Vec::new();
    for c in directive().captures_iter(text) {
        let m = c.get(0).expect("match");
        if file.knows_comments() && file.kind_at(m.start() + m.as_str().find('#').unwrap_or(0)) != Some(SegKind::Code) {
            continue;
        }
        match &c[1] {
            "if" | "ifdef" | "ifndef" => {
                let guard = &c[1] == "ifndef"
                    && c.get(2).is_some_and(|name| {
                        let after = &text[m.end()..];
                        let next = after.trim_start_matches(['\r', '\n', ' ', '\t']);
                        next.strip_prefix("#define")
                            .map(|r| r.trim_start().starts_with(name.as_str()))
                            .unwrap_or(false)
                            && text[..m.start()].trim().lines().all(|l| {
                                l.trim().is_empty()
                                    || l.trim_start().starts_with("//")
                                    || l.trim_start().starts_with("/*")
                                    || l.trim_start().starts_with('*')
                            })
                    });
                stack.push((m.start(), guard));
            }
            "endif" => {
                if let Some((start, guard)) = stack.pop() {
                    if !guard {
                        out.push((start, m.end()));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn deepest(file: &SourceFile, start: usize, end: usize) -> Option<Node<'_>> {
    let root = file.root()?;
    root.descendant_for_byte_range(start, end.max(start))
}

/// Why an edit of `[start, end)` cannot be verified by building the project,
/// if it cannot. `repairs_syntax` is true for `repaired` rules, whose purpose
/// is to fix code that does not parse; they may edit damaged regions (the
/// session still refuses any new syntax error).
pub fn unverifiable(file: &SourceFile, start: usize, end: usize, repairs_syntax: bool) -> Option<String> {
    let damage = if repairs_syntax {
        Vec::new()
    } else {
        file.parse_damage()
    };
    for (a, b) in damage {
        // A MISSING node is empty; treat it as covering its position.
        let (a, b) = if a == b { (a.saturating_sub(1), b + 1) } else { (a, b) };
        let touches = if start == end {
            a < start && start < b
        } else {
            start < b && a < end
        };
        if touches {
            return Some(format!(
                "the grammar could not parse the code here (line {}), so its structure is unknown",
                file.line_of(a)
            ));
        }
    }
    for (a, b) in conditional_regions(file) {
        let inside = if start == end {
            a < start && start < b
        } else {
            start < b && a < end
        };
        if inside {
            return Some(format!(
                "the code is inside a conditional-compilation block (line {}): a build checks only one configuration, so the correction could not be verified",
                file.line_of(a)
            ));
        }
    }
    if file.lang.map(|l| l.id()) == Some("rust") {
        let mut node = deepest(file, start, end);
        while let Some(n) = node {
            // Strictly inside a macro body: token trees are not parsed as code.
            if matches!(n.kind(), "token_tree" | "macro_definition" | "macro_rule")
                && n.start_byte() < start
                && end < n.end_byte()
            {
                return Some(format!(
                    "the code is inside a macro (line {}), whose body is not parsed as code until the macro expands",
                    file.line_of(n.start_byte())
                ));
            }
            // An item behind #[cfg(...)] other than #[cfg(test)].
            let mut prev = n.prev_named_sibling();
            while let Some(p) = prev.filter(|p| p.kind() == "attribute_item") {
                let attr = &file.text[p.byte_range()];
                let compact: String = attr.chars().filter(|c| !c.is_whitespace()).collect();
                if compact.starts_with("#[cfg(") && compact != "#[cfg(test)]" {
                    return Some(format!(
                        "the code is behind `{}` (line {}): a build checks only one configuration, so the correction could not be verified",
                        attr.trim(),
                        file.line_of(p.start_byte())
                    ));
                }
                prev = p.prev_named_sibling();
            }
            node = n.parent();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_conditionals_but_not_include_guards() {
        let src = "#ifndef CART_H\n#define CART_H\nint a;\n#ifdef DEBUG\nint b;\n#endif\nint c;\n#endif\n";
        let f = SourceFile::new("cart.h", src.to_string());
        let regions = conditional_regions(&f);
        assert_eq!(regions.len(), 1, "{regions:?}");
        let b = src.find("int b").unwrap();
        let a = src.find("int a").unwrap();
        assert!(unverifiable(&f, b, b + 5, false).is_some());
        assert!(unverifiable(&f, a, a + 5, false).is_none());
    }

    #[test]
    fn unparsed_regions_are_left_alone() {
        let src = "def f(:\n    return 1\n\nx = 1\n";
        let f = SourceFile::new("a.py", src.to_string());
        assert!(!f.parse_damage().is_empty());
        let r = src.find("return").unwrap();
        assert!(unverifiable(&f, r, r + 6, false).is_some_and(|m| m.contains("could not parse")));
        let x = src.find("x = 1").unwrap();
        assert!(unverifiable(&f, x, x + 5, false).is_none());
    }

    #[test]
    fn swift_flat_directives() {
        let src = "func f() {\n#if DEBUG\n    print(1)\n#endif\n    print(2)\n}\n";
        let f = SourceFile::new("a.swift", src.to_string());
        let one = src.find("print(1)").unwrap();
        let two = src.find("print(2)").unwrap();
        assert!(unverifiable(&f, one, one + 8, false).is_some());
        assert!(unverifiable(&f, two, two + 8, false).is_none());
    }

    #[test]
    fn rust_cfg_and_macros() {
        let src = "#[cfg(windows)]\nfn win() -> () { return; }\n#[cfg(test)]\nfn t() -> () {}\nmacro_rules! m { () => { return 1; } }\nfn ok() -> () {}\n";
        let f = SourceFile::new("lib.rs", src.to_string());
        let w = src.find("return;").unwrap();
        assert!(unverifiable(&f, w, w + 7, false).unwrap().contains("cfg(windows)"));
        let t = src.find("fn t").unwrap() + 7;
        assert!(
            unverifiable(&f, t, t + 5, false).is_none(),
            "cfg(test) is built by `cargo test`"
        );
        let m = src.find("return 1").unwrap();
        assert!(unverifiable(&f, m, m + 8, false).unwrap().contains("macro"));
        let ok = src.find("fn ok").unwrap() + 8;
        assert!(unverifiable(&f, ok, ok + 5, false).is_none());
    }
}
