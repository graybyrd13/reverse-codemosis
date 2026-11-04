//! Inline suppressions: `codemosis: ignore[rule] reason`.
//!
//! A person can say "I know, and here is why" next to the code:
//!
//! ```text
//! except:  # codemosis: ignore[python-core/bare-except] the worker must never die; errors are logged above
//! # codemosis: ignore[python-core/bare-except] until=2027-01-31 owner=@payments
//! # codemosis: ignore-file[universal/private-key] test fixture, not a real key
//! ```
//!
//! * `ignore[...]` covers findings that start on its own line, and on the
//!   next line when the comment is alone on its line.
//! * `ignore-file[...]` covers the whole file.
//! * The brackets list rule ids, pack ids (`python-core`) or `*`.
//! * `until=YYYY-MM-DD` ends the suppression after that day: the finding
//!   comes back, with a note that the suppression expired.
//! * `owner=...` records who answers for it. The rest is the reason.
//!
//! In files whose comment syntax is known the marker only counts inside a
//! comment, so a string that happens to contain it changes nothing.

use crate::lex::SegKind;
use crate::model::{Evidence, Finding, Span, Status, Suppression, SuppressionKind};
use crate::project::{Project, SourceFile};
use regex::Regex;
use std::sync::OnceLock;

/// One `codemosis: ignore` marker found in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub line: usize,
    pub whole_file: bool,
    /// The comment is alone on its line, so it also covers the next line.
    pub own_line: bool,
    pub rules: Vec<String>,
    pub reason: Option<String>,
    pub until: Option<String>,
    pub owner: Option<String>,
    pub span: Span,
}

impl Marker {
    pub fn covers_rule(&self, rule: &str) -> bool {
        self.rules
            .iter()
            .any(|r| r == "*" || r == rule || rule.starts_with(&format!("{r}/")))
    }

    fn covers_line(&self, line: usize) -> bool {
        self.whole_file || line == self.line || (self.own_line && line == self.line + 1)
    }
}

fn marker_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"codemosis:\s*ignore(?P<file>-file)?\[(?P<rules>[^\]\n]*)\](?P<rest>[^\n]*)").expect("valid regex")
    })
}

fn date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid regex"))
}

/// Every marker in a file.
pub fn markers(file: &SourceFile) -> Vec<Marker> {
    let text = &file.text;
    let mut out = Vec::new();
    for c in marker_re().captures_iter(text) {
        let m = c.get(0).expect("whole match");
        if file.knows_comments() && file.kind_at(m.start()) != Some(SegKind::Comment) {
            continue;
        }
        let line_start = text[..m.start()].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let before = &text[line_start..m.start()];
        let own_line = match file.knows_comments() {
            true => before
                .char_indices()
                .find(|(_, ch)| !ch.is_whitespace())
                .is_none_or(|(i, _)| file.kind_at(line_start + i) == Some(SegKind::Comment)),
            false => !before.chars().any(|ch| ch.is_alphanumeric()) || before.trim().eq_ignore_ascii_case("rem"),
        };
        let mut rest = c["rest"].trim().to_string();
        for closer in ["*/", "-->", "--%>", "%>", "#}", "}}", "-}", "*)", "|#"] {
            if let Some(stripped) = rest.strip_suffix(closer) {
                rest = stripped.trim_end().to_string();
            }
        }
        let mut until = None;
        let mut owner = None;
        let mut words = Vec::new();
        for w in rest.split_whitespace() {
            if let Some(d) = w.strip_prefix("until=") {
                until = Some(d.to_string());
            } else if let Some(o) = w.strip_prefix("owner=") {
                owner = Some(o.to_string());
            } else {
                words.push(w);
            }
        }
        let reason = words
            .join(" ")
            .trim_start_matches(['-', '—', ':', ' '])
            .trim()
            .to_string();
        out.push(Marker {
            line: file.line_of(m.start()),
            whole_file: c.name("file").is_some(),
            own_line,
            rules: c["rules"]
                .split([',', ' '])
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(String::from)
                .collect(),
            reason: (!reason.is_empty()).then_some(reason),
            until,
            owner,
            span: Span::from_range(text, m.start(), m.end()),
        });
    }
    out
}

/// Today as YYYY-MM-DD (UTC), or `$CODEMOSIS_TODAY` when set (for tests
/// and reproducible runs).
pub fn today() -> String {
    if let Ok(d) = std::env::var("CODEMOSIS_TODAY") {
        if date_re().is_match(&d) {
            return d;
        }
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_date(secs / 86_400)
}

/// The proleptic Gregorian date of a day count since 1970-01-01 (Howard
/// Hinnant's algorithm).
pub fn civil_date(days: u64) -> String {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Whether a suppression ending on `until` still applies on `today`. A
/// malformed date never applies, so a typo cannot hide a finding forever.
pub fn still_applies(until: Option<&str>, today: &str) -> Result<(), String> {
    match until {
        None => Ok(()),
        Some(d) if !date_re().is_match(d) => Err(format!("its date '{d}' is not YYYY-MM-DD")),
        Some(d) if d < today => Err(format!("it expired on {d}")),
        Some(_) => Ok(()),
    }
}

/// Mark findings covered by an inline marker as suppressed. A marker that
/// has expired leaves the finding open and says so in its evidence.
pub fn apply_inline(project: &Project, findings: &mut [Finding], today: &str) {
    let mut cache: Vec<Option<Vec<Marker>>> = vec![None; project.files.len()];
    for f in findings.iter_mut() {
        if !f.is_open() {
            continue;
        }
        let Some(idx) = project.index_of(&f.file) else {
            continue;
        };
        let markers = cache[idx].get_or_insert_with(|| markers(&project.files[idx]));
        let Some(m) = markers
            .iter()
            .find(|m| m.covers_rule(&f.rule) && m.covers_line(f.span.start.line))
        else {
            continue;
        };
        match still_applies(m.until.as_deref(), today) {
            Ok(()) => {
                f.status = Status::Suppressed {
                    suppression: Suppression {
                        kind: SuppressionKind::InSource,
                        reason: m.reason.clone(),
                        until: m.until.clone(),
                        owner: m.owner.clone(),
                        at: format!("{}:{}", f.file.display(), m.line),
                    },
                };
                f.edits.clear();
            }
            Err(why) => f.evidence.push(Evidence {
                span: m.span.clone(),
                label: format!("a suppression here no longer applies: {why}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(19_723), "2024-01-01");
        assert_eq!(civil_date(20_759), "2026-11-02");
        assert!(still_applies(Some("2026-12-31"), "2026-11-04").is_ok());
        assert!(still_applies(Some("2026-11-03"), "2026-11-04").is_err());
        assert!(still_applies(Some("next week"), "2026-11-04").is_err());
    }

    #[test]
    fn parses_markers() {
        let f = SourceFile::new(
            "a.py",
            "x = 1  # codemosis: ignore[python-core/is-literal, waits] legacy until=2027-01-01 owner=@ops\n\
             # codemosis: ignore-file[*]\n\
             s = \"codemosis: ignore[*] not a comment\"\n"
                .to_string(),
        );
        let ms = markers(&f);
        assert_eq!(ms.len(), 2, "{ms:?}");
        assert_eq!(ms[0].rules, vec!["python-core/is-literal", "waits"]);
        assert_eq!(ms[0].reason.as_deref(), Some("legacy"));
        assert_eq!(ms[0].until.as_deref(), Some("2027-01-01"));
        assert_eq!(ms[0].owner.as_deref(), Some("@ops"));
        assert!(!ms[0].own_line);
        assert!(ms[0].covers_rule("waits/fixed-wait"));
        assert!(!ms[0].covers_rule("python-core/bare-except"));
        assert!(ms[1].whole_file && ms[1].own_line);
    }

    #[test]
    fn block_comment_closers_are_not_reasons() {
        let f = SourceFile::new(
            "a.c",
            "/* codemosis: ignore[c-family] generated by hand */\nint x;\n".to_string(),
        );
        let ms = markers(&f);
        assert_eq!(ms[0].reason.as_deref(), Some("generated by hand"));
        assert!(ms[0].own_line && ms[0].covers_line(2));
    }
}
