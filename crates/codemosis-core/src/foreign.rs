//! Suppressions written for other tools.
//!
//! Projects already record decisions for their linters: `# noqa: F821`,
//! `# type: ignore[name-defined]`, `// eslint-disable-next-line no-console`,
//! `#[allow(clippy::needless_return)]`. When a rule declares that it finds
//! the same thing as one of those checks (`equivalents = ["flake8:F821"]`),
//! the engine honours the existing suppression instead of asking again.
//!
//! Recognised forms, each only inside a comment (or, for Rust, an attribute):
//!
//! | Tool | Same line | Next line | Wider |
//! |---|---|---|---|
//! | flake8, ruff | `# noqa`, `# noqa: F821,E501` | | `# flake8: noqa`, `# ruff: noqa` (file) |
//! | mypy | `# type: ignore`, `# type: ignore[name-defined]` | | |
//! | pyright | `# pyright: ignore[reportUndefinedVariable]` | | |
//! | pylint | `# pylint: disable=undefined-variable` | | |
//! | eslint | `// eslint-disable-line no-console` | `// eslint-disable-next-line ...` | `/* eslint-disable ... */` to `/* eslint-enable */` |
//! | clang-tidy | `// NOLINT(check)` | `// NOLINTNEXTLINE(check)` | |
//! | golangci-lint | `//nolint:gosimple` | | |
//! | shellcheck | | `# shellcheck disable=SC2086` | |
//! | clippy | | | `#[allow(clippy::x)]` on an item, `#![allow(clippy::x)]` (file) |
//!
//! A bare form (`# noqa`, `NOLINT`) covers every rule with an equivalent in
//! that tool; a list covers only the codes listed. Group names (`clippy::all`)
//! are not expanded: when in doubt, the finding is reported.

use crate::lex::SegKind;
use crate::project::SourceFile;
use regex::Regex;
use std::sync::OnceLock;

/// One suppression written for another tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignMarker {
    /// The tool, as rules name it in `equivalents` (`flake8`, `eslint`...).
    pub tool: &'static str,
    /// The codes it suppresses; `None` means every check of the tool.
    pub codes: Option<Vec<String>>,
    /// Lines it covers (1-based, inclusive).
    pub first_line: usize,
    pub last_line: usize,
    /// The marker's own line, for reports.
    pub line: usize,
    /// The marker as written, for reports.
    pub text: String,
}

impl ForeignMarker {
    /// Whether this marker covers an equivalent such as `flake8:F821`.
    pub fn covers(&self, equivalent: &str, line: usize) -> bool {
        let Some((tool, code)) = equivalent.split_once(':') else {
            return false;
        };
        let tool_matches =
            self.tool == tool || (self.tool == "flake8" && tool == "ruff") || (self.tool == "ruff" && tool == "flake8");
        tool_matches
            && line >= self.first_line
            && line <= self.last_line
            && self
                .codes
                .as_ref()
                .is_none_or(|codes| codes.iter().any(|c| c.eq_ignore_ascii_case(code)))
    }
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid regex"))
}

fn split_codes(s: &str) -> Option<Vec<String>> {
    let codes: Vec<String> = s
        .split([',', ' ', '\t', '\n'])
        .map(str::trim)
        .filter(|c| !c.is_empty() && *c != "--")
        .take_while(|c| !c.starts_with("--"))
        .map(String::from)
        .collect();
    (!codes.is_empty()).then_some(codes)
}

/// Every foreign suppression in a file.
pub fn markers(file: &SourceFile) -> Vec<ForeignMarker> {
    static NOQA: OnceLock<Regex> = OnceLock::new();
    static NOQA_FILE: OnceLock<Regex> = OnceLock::new();
    static TYPE_IGNORE: OnceLock<Regex> = OnceLock::new();
    static PYRIGHT: OnceLock<Regex> = OnceLock::new();
    static PYLINT: OnceLock<Regex> = OnceLock::new();
    static ESLINT_LINE: OnceLock<Regex> = OnceLock::new();
    static ESLINT_BLOCK: OnceLock<Regex> = OnceLock::new();
    static NOLINT: OnceLock<Regex> = OnceLock::new();
    static GOLANGCI: OnceLock<Regex> = OnceLock::new();
    static SHELLCHECK: OnceLock<Regex> = OnceLock::new();

    let text = &file.text;
    let lines = text.matches('\n').count() + 1;
    let in_comment = |offset: usize| !file.knows_comments() || file.kind_at(offset) == Some(SegKind::Comment);
    let mut out = Vec::new();
    let mut push = |tool: &'static str, codes: Option<Vec<String>>, at: usize, first: usize, last: usize, raw: &str| {
        out.push(ForeignMarker {
            tool,
            codes,
            first_line: first,
            last_line: last,
            line: file.line_of(at),
            text: raw.trim().to_string(),
        });
    };

    let line_patterns: [(&'static str, &Regex, bool); 6] = [
        (
            "flake8",
            re(
                &NOQA,
                r"(?i)#\s*noqa(?:\s*:\s*(?P<codes>[A-Z]+[0-9]+(?:[,\s]+[A-Z]+[0-9]+)*))?",
            ),
            false,
        ),
        (
            "mypy",
            re(&TYPE_IGNORE, r"#\s*type:\s*ignore(?:\[(?P<codes>[^\]]*)\])?"),
            false,
        ),
        (
            "pyright",
            re(&PYRIGHT, r"#\s*pyright:\s*ignore(?:\[(?P<codes>[^\]]*)\])?"),
            false,
        ),
        (
            "pylint",
            re(
                &PYLINT,
                r"#[ \t]*pylint:[ \t]*disable[ \t]*=[ \t]*(?P<codes>[\w,\t -]+)",
            ),
            false,
        ),
        (
            "clang-tidy",
            re(&NOLINT, r"\bNOLINT(?P<next>NEXTLINE)?\b(?:\((?P<codes>[^)]*)\))?"),
            false,
        ),
        ("golangci", re(&GOLANGCI, r"//\s*nolint(?::(?P<codes>[\w,-]+))?"), false),
    ];
    for (tool, pattern, _) in line_patterns {
        for c in pattern.captures_iter(text) {
            let m = c.get(0).expect("match");
            if !in_comment(m.start()) {
                continue;
            }
            let line = file.line_of(m.start());
            let next = c.name("next").is_some();
            let (first, last) = if next { (line + 1, line + 1) } else { (line, line) };
            push(
                tool,
                c.name("codes").and_then(|x| split_codes(x.as_str())),
                m.start(),
                first,
                last,
                m.as_str(),
            );
        }
    }
    for c in re(
        &NOQA_FILE,
        r"(?im)^\s*#\s*(?:flake8|ruff)\s*:\s*noqa(?:\s*:\s*(?P<codes>[A-Z0-9,\s]+))?\s*$",
    )
    .captures_iter(text)
    {
        let m = c.get(0).expect("match");
        if in_comment(m.start() + m.as_str().find('#').unwrap_or(0)) {
            push(
                "flake8",
                c.name("codes").and_then(|x| split_codes(x.as_str())),
                m.start(),
                1,
                lines,
                m.as_str(),
            );
        }
    }
    for c in re(
        &ESLINT_LINE,
        r"eslint-disable-(?P<which>next-line|line)(?P<codes>[ \t]+[^\n*]*)?",
    )
    .captures_iter(text)
    {
        let m = c.get(0).expect("match");
        if !in_comment(m.start()) {
            continue;
        }
        let line = file.line_of(m.start());
        let target = if &c["which"] == "next-line" { line + 1 } else { line };
        push(
            "eslint",
            c.name("codes").and_then(|x| split_codes(x.as_str())),
            m.start(),
            target,
            target,
            m.as_str(),
        );
    }
    // /* eslint-disable rules */ ... /* eslint-enable */ (or the end of the file).
    let blocks: Vec<(usize, Option<Vec<String>>, bool)> = re(
        &ESLINT_BLOCK,
        r"eslint-(?P<what>disable|enable)(?P<codes>[ \t]+[^\n*\s-][^\n*]*?)?[ \t]*(?:\*/|\n|$)",
    )
    .captures_iter(text)
    .filter_map(|c| {
        let m = c.get(0).expect("match");
        let after = &text[m.start() + "eslint-".len()..];
        if after.starts_with("disable-") || !in_comment(m.start()) {
            return None;
        }
        Some((
            m.start(),
            c.name("codes").and_then(|x| split_codes(x.as_str())),
            &c["what"] == "disable",
        ))
    })
    .collect();
    for (i, (at, codes, disable)) in blocks.iter().enumerate() {
        if !disable {
            continue;
        }
        let end = blocks[i + 1..]
            .iter()
            .find(|(_, _, d)| !d)
            .map(|(e, _, _)| file.line_of(*e))
            .unwrap_or(lines);
        push("eslint", codes.clone(), *at, file.line_of(*at), end, "eslint-disable");
    }
    // /* eslint no-empty: 0 */ or "off": an inline configuration, which
    // applies to the whole file.
    static ESLINT_CONFIG: OnceLock<Regex> = OnceLock::new();
    for c in re(
        &ESLINT_CONFIG,
        r#"/\*\s*eslint\s+(?P<rules>(?:[\w@/-]+\s*:\s*(?:0|"off"|'off'|off)\s*,?\s*)+)\*/"#,
    )
    .captures_iter(text)
    {
        let m = c.get(0).expect("match");
        if !in_comment(m.start()) {
            continue;
        }
        let codes: Vec<String> = c["rules"]
            .split(',')
            .filter_map(|r| r.split(':').next().map(|n| n.trim().to_string()))
            .filter(|n| !n.is_empty())
            .collect();
        if !codes.is_empty() {
            push("eslint", Some(codes), m.start(), 1, lines, m.as_str());
        }
    }
    for c in re(&SHELLCHECK, r"#\s*shellcheck\s+disable\s*=\s*(?P<codes>[\w,]+)").captures_iter(text) {
        let m = c.get(0).expect("match");
        if in_comment(m.start()) {
            let line = file.line_of(m.start());
            push(
                "shellcheck",
                split_codes(&c["codes"]),
                m.start(),
                line,
                line + 1,
                m.as_str(),
            );
        }
    }
    if file.lang.map(|l| l.id()) == Some("rust") {
        out.extend(rust_allows(file, lines));
    }
    out
}

/// `#[allow(clippy::x)]` on an item covers the item; `#![allow(clippy::x)]`
/// covers the file.
fn rust_allows(file: &SourceFile, lines: usize) -> Vec<ForeignMarker> {
    static ALLOW: OnceLock<Regex> = OnceLock::new();
    let allow = re(
        &ALLOW,
        r"#(?P<inner>!)?\[\s*(?:allow|expect)\s*\((?P<lints>[^\]]*)\)\s*\]",
    );
    let mut out = Vec::new();
    let Some(root) = file.root() else { return out };
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "attribute_item" | "inner_attribute_item") {
            let raw = &file.text[n.byte_range()];
            if let Some(c) = allow.captures(raw) {
                let codes: Vec<String> = c["lints"]
                    .split(',')
                    .filter_map(|l| l.trim().strip_prefix("clippy::").map(String::from))
                    .collect();
                if !codes.is_empty() {
                    let (first, last) = if c.name("inner").is_some() {
                        (1, lines)
                    } else {
                        // The attribute applies to the next item.
                        let mut item = n.next_named_sibling();
                        while let Some(i) = item.filter(|i| i.kind() == "attribute_item" || i.is_extra()) {
                            item = i.next_named_sibling();
                        }
                        match item {
                            Some(i) => (file.line_of(n.start_byte()), file.line_of(i.end_byte())),
                            None => continue,
                        }
                    };
                    out.push(ForeignMarker {
                        tool: "clippy",
                        codes: Some(codes),
                        first_line: first,
                        last_line: last,
                        line: file.line_of(n.start_byte()),
                        text: raw.trim().to_string(),
                    });
                }
            }
        }
        let mut cursor = n.walk();
        stack.extend(n.named_children(&mut cursor));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covers(file: &SourceFile, equivalent: &str, line: usize) -> bool {
        markers(file).iter().any(|m| m.covers(equivalent, line))
    }

    #[test]
    fn python_forms() {
        let f = SourceFile::new(
            "a.py",
            "x = undefined_a  # noqa: F821\ny = undefined_b  # type: ignore[name-defined]\nz = c  # noqa\nw = d  # pylint: disable=undefined-variable\ns = '# noqa: F821'\n"
                .to_string(),
        );
        assert!(covers(&f, "flake8:F821", 1));
        assert!(covers(&f, "ruff:F821", 1));
        assert!(!covers(&f, "flake8:F401", 1));
        assert!(covers(&f, "mypy:name-defined", 2));
        assert!(!covers(&f, "mypy:name-defined", 1));
        assert!(covers(&f, "flake8:F401", 3), "a bare noqa covers every flake8 code");
        assert!(covers(&f, "pylint:undefined-variable", 4));
        assert!(!covers(&f, "flake8:F821", 5), "a marker in a string is not a marker");
    }

    #[test]
    fn javascript_forms() {
        let f = SourceFile::new(
            "a.mjs",
            "console.log(1); // eslint-disable-line no-console\n// eslint-disable-next-line no-console, no-debugger\nconsole.log(2);\n/* eslint-disable no-empty */\ntry {} catch {}\n/* eslint-enable */\ntry {} catch {}\n"
                .to_string(),
        );
        assert!(covers(&f, "eslint:no-console", 1));
        assert!(covers(&f, "eslint:no-console", 3));
        assert!(!covers(&f, "eslint:no-console", 2));
        assert!(covers(&f, "eslint:no-empty", 5));
        assert!(!covers(&f, "eslint:no-empty", 7));
        let g = SourceFile::new(
            "b.mjs",
            "try { f(); } catch (e) {}\n/*eslint no-empty:0*/\n".to_string(),
        );
        assert!(
            covers(&g, "eslint:no-empty", 1),
            "inline configuration applies to the file"
        );
    }

    #[test]
    fn rust_allow_attributes() {
        let src = "#[allow(clippy::needless_return)]\nfn a() -> i32 {\n    return 1;\n}\n\nfn b() -> i32 {\n    return 2;\n}\n";
        let f = SourceFile::new("lib.rs", src.to_string());
        assert!(covers(&f, "clippy:needless_return", 3));
        assert!(!covers(&f, "clippy:needless_return", 7));
        let g = SourceFile::new("lib.rs", format!("#![allow(clippy::needless_return)]\n{src}"));
        assert!(covers(&g, "clippy:needless_return", 8));
    }

    #[test]
    fn other_tools() {
        let c = SourceFile::new("a.c", "int f(int x) {\n  // NOLINTNEXTLINE(bugprone-assignment-in-if-condition)\n  if (x = 1) return 1;\n  return 0;\n}\n".to_string());
        assert!(covers(&c, "clang-tidy:bugprone-assignment-in-if-condition", 3));
        let sh = SourceFile::new(
            "a.sh",
            "#!/bin/sh\n# shellcheck disable=SC2086\nrm -rf $DIR\n".to_string(),
        );
        assert!(covers(&sh, "shellcheck:SC2086", 3));
        let go = SourceFile::new(
            "a.go",
            "package a\n\nfunc f(b bool) bool {\n\treturn b == true //nolint:gosimple\n}\n".to_string(),
        );
        assert!(covers(&go, "golangci:gosimple", 4));
    }
}
