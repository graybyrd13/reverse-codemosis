//! Version requirements, for packs that declare which engines they need.
//!
//! A requirement is a comma-separated list of comparators, all of which must
//! hold: `>=0.1`, `>=0.1.0, <0.3`, `=0.2.1`, `^0.2` (compatible: same
//! leftmost non-zero component), `~0.2.1` (same minor). Missing components
//! are zero.

use std::cmp::Ordering;
use std::fmt;

/// The engine version packs are checked against.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Version {
    /// Parse `1`, `1.2` or `1.2.3` (a pre-release or build suffix is ignored).
    pub fn parse(s: &str) -> Result<(Version, usize), String> {
        let core = s.trim().split(['-', '+']).next().unwrap_or("");
        let parts: Vec<&str> = core.split('.').collect();
        if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
            return Err(format!("'{s}' is not a version like 1.2.3"));
        }
        let mut n = [0u64; 3];
        for (i, p) in parts.iter().enumerate() {
            n[i] = p.parse().map_err(|_| format!("'{s}' is not a version like 1.2.3"))?;
        }
        Ok((
            Version {
                major: n[0],
                minor: n[1],
                patch: n[2],
            },
            parts.len(),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Ge,
    Gt,
    Le,
    Lt,
    Eq,
    Caret,
    Tilde,
}

/// A parsed requirement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Requirement {
    source: String,
    comparators: Vec<(Op, Version, usize)>,
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

impl Requirement {
    pub fn parse(s: &str) -> Result<Requirement, String> {
        let mut comparators = Vec::new();
        for part in s.split(',') {
            let part = part.trim();
            let (op, rest) = [
                (">=", Op::Ge),
                ("<=", Op::Le),
                (">", Op::Gt),
                ("<", Op::Lt),
                ("=", Op::Eq),
                ("^", Op::Caret),
                ("~", Op::Tilde),
            ]
            .iter()
            .find_map(|(p, op)| part.strip_prefix(p).map(|r| (*op, r)))
            .unwrap_or((Op::Caret, part));
            let (v, given) = Version::parse(rest)?;
            comparators.push((op, v, given));
        }
        if comparators.is_empty() {
            return Err("an empty version requirement".to_string());
        }
        Ok(Requirement {
            source: s.trim().to_string(),
            comparators,
        })
    }

    pub fn matches(&self, v: Version) -> bool {
        self.comparators.iter().all(|(op, r, given)| {
            let ord = v.cmp(r);
            match op {
                Op::Ge => ord != Ordering::Less,
                Op::Gt => ord == Ordering::Greater,
                Op::Le => ord != Ordering::Greater,
                Op::Lt => ord == Ordering::Less,
                Op::Eq => ord == Ordering::Equal,
                Op::Tilde => ord != Ordering::Less && v.major == r.major && (*given < 2 || v.minor == r.minor),
                Op::Caret => {
                    ord != Ordering::Less
                        && if r.major > 0 || *given == 1 {
                            v.major == r.major
                        } else if r.minor > 0 || *given == 2 {
                            v.major == 0 && v.minor == r.minor
                        } else {
                            v.major == 0 && v.minor == 0 && v.patch == r.patch
                        }
                }
            }
        })
    }
}

pub fn engine() -> Version {
    Version::parse(ENGINE_VERSION).expect("the crate version parses").0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap().0
    }

    #[test]
    fn requirements() {
        let r = Requirement::parse(">=0.1, <0.3").unwrap();
        assert!(r.matches(v("0.1.0")) && r.matches(v("0.2.9")) && !r.matches(v("0.3.0")));
        assert!(Requirement::parse("^0.2").unwrap().matches(v("0.2.7")));
        assert!(!Requirement::parse("^0.2").unwrap().matches(v("0.3.0")));
        assert!(Requirement::parse("^1.2").unwrap().matches(v("1.9.0")));
        assert!(!Requirement::parse("^1.2").unwrap().matches(v("2.0.0")));
        assert!(Requirement::parse("~0.2.1").unwrap().matches(v("0.2.5")));
        assert!(!Requirement::parse("~0.2.1").unwrap().matches(v("0.3.0")));
        assert!(Requirement::parse("=0.1.0").unwrap().matches(v("0.1.0")));
        assert!(Requirement::parse("0.1").unwrap().matches(v("0.1.4")));
        assert!(Requirement::parse(">= banana").is_err());
        assert!(Requirement::parse(&format!("={ENGINE_VERSION}"))
            .unwrap()
            .matches(engine()));
    }
}
