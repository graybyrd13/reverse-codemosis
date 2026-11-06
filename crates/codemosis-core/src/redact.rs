//! Keeping secrets out of what the engine prints.
//!
//! Findings, questions, source excerpts and diffs end up in terminals, CI
//! logs, pull-request comments and SARIF uploads. Anything that looks like a
//! credential is masked before it leaves the engine: the recognizable prefix
//! stays (so `ghp_` still says "GitHub token") and every other character
//! becomes `*`, which keeps columns aligned in excerpts.
//!
//! Masking is deliberately broad: a false positive costs a few asterisks in a
//! report; a false negative leaks a key.

use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// Patterns whose `secret` group is masked.
const PATTERNS: &[(&str, &str)] = &[
    (
        "private key block",
        r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY(?: BLOCK)?-----(?P<secret>.*?)-----END",
    ),
    ("AWS access key id", r"\b(?:AKIA|ASIA)(?P<secret>[0-9A-Z]{16})\b"),
    (
        "AWS secret access key",
        r#"(?i)aws_?secret_?access_?key["']?\s*[:=]\s*["']?(?P<secret>[A-Za-z0-9/+=]{40})"#,
    ),
    (
        "GitHub token",
        r"\b(?:ghp|gho|ghu|ghs|ghr)_(?P<secret>[A-Za-z0-9]{30,})",
    ),
    (
        "GitHub fine-grained token",
        r"\bgithub_pat_(?P<secret>[A-Za-z0-9_]{22,})",
    ),
    ("GitLab token", r"\bglpat-(?P<secret>[A-Za-z0-9_-]{20,})"),
    ("Slack token", r"\bxox[abprs]-(?P<secret>[A-Za-z0-9-]{10,})"),
    (
        "Stripe key",
        r"\b(?:sk|rk|pk)_(?:live|test)_(?P<secret>[A-Za-z0-9]{16,})",
    ),
    ("Google API key", r"\bAIza(?P<secret>[0-9A-Za-z_-]{35})"),
    ("npm token", r"\bnpm_(?P<secret>[A-Za-z0-9]{36})"),
    ("API key (sk-)", r"\bsk-(?:ant-|proj-)?(?P<secret>[A-Za-z0-9_-]{32,})"),
    (
        "JSON Web Token",
        r"\beyJ[A-Za-z0-9_-]{8,}\.(?P<secret>eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})",
    ),
    (
        "credential in a URL",
        r"\b[a-z][a-z0-9+.-]*://[^/\s:@]+:(?P<secret>[^@\s/]{3,})@",
    ),
    (
        "assigned credential",
        r#"(?i)(?:password|passwd|pwd|secret|client_secret|api[_-]?key|access[_-]?key|auth[_-]?token|access[_-]?token|private[_-]?key)["']?\s*[:=]\s*["'](?P<secret>[^"'\s]{8,})["']"#,
    ),
];

fn compiled() -> &'static [(&'static str, Regex)] {
    static CELL: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    CELL.get_or_init(|| {
        PATTERNS
            .iter()
            .map(|(name, p)| (*name, Regex::new(p).expect("valid redaction pattern")))
            .collect()
    })
}

/// The kinds of secret found in `text`, for tests and diagnostics.
pub fn kinds(text: &str) -> Vec<&'static str> {
    compiled()
        .iter()
        .filter(|(_, re)| re.is_match(text))
        .map(|(name, _)| *name)
        .collect()
}

/// `text` with every secret masked. Idempotent: masking can expose a match
/// a previous mask overlapped (a token pattern swallowing the `-----END` of a
/// key block, say), so passes repeat until nothing changes. Each pass only
/// turns characters into `*`, so this ends.
pub fn redact(text: &str) -> Cow<'_, str> {
    let mut current = match mask_once(text) {
        None => return Cow::Borrowed(text),
        Some(t) => t,
    };
    for _ in 0..8 {
        match mask_once(&current) {
            Some(next) if next != current => current = next,
            _ => break,
        }
    }
    Cow::Owned(current)
}

fn mask_once(text: &str) -> Option<String> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (_, re) in compiled() {
        for c in re.captures_iter(text) {
            if let Some(m) = c.name("secret") {
                ranges.push((m.start(), m.end()));
            }
        }
    }
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_unstable();
    let mut out = String::with_capacity(text.len());
    let mut line_start = true;
    for (i, ch) in text.char_indices() {
        let secret = ranges.iter().any(|(a, b)| i >= *a && i < *b);
        // Keep whitespace and line structure, and a diff's leading marker.
        let keep = !secret || ch.is_whitespace() || (line_start && matches!(ch, '+' | '-' | ' '));
        out.push(if keep { ch } else { '*' });
        line_start = ch == '\n';
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_known_formats_and_keeps_prefixes() {
        let r = redact("token = ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        assert!(r.contains("ghp_****"), "{r}");
        assert!(!r.contains("abcdef"), "{r}");
        let r = redact("key: AKIAIOSFODNN7EXAMPLE end");
        assert_eq!(r, "key: AKIA**************** end");
        let r = redact(r#"password = "correct horse""#);
        assert_eq!(r, r#"password = "correct horse""#, "spaces end a credential value");
        let r = redact(r#"DB_PASSWORD="s3cr3t-value!""#);
        assert_eq!(r, r#"DB_PASSWORD="*************""#);
        let r = redact("postgres://admin:hunter22@db.example.com/app");
        assert_eq!(r, "postgres://admin:********@db.example.com/app");
    }

    #[test]
    fn masks_private_key_bodies_in_diffs() {
        // codemosis: ignore[universal/private-key] a test fixture for redaction, not a key
        let diff = "+-----BEGIN RSA PRIVATE KEY-----\n+MIIEowIBAAKCAQEA+xyz\n+-----END RSA PRIVATE KEY-----\n";
        let r = redact(diff);
        // codemosis: ignore[universal/private-key] a test fixture for redaction, not a key
        assert!(r.contains("+-----BEGIN RSA PRIVATE KEY-----"), "{r}");
        assert!(r.contains("\n+********************\n"), "{r}");
        assert!(!r.contains("MIIE"), "{r}");
    }

    #[test]
    fn leaves_ordinary_code_alone() {
        let code = "def total(items):\n    return sum(i.price for i in items)  # sk-free\n";
        assert!(matches!(redact(code), Cow::Borrowed(_)));
        assert!(kinds(code).is_empty());
    }
}
