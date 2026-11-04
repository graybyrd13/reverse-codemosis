//! Applying edits to text, safely.

use crate::model::Edit;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EditError {
    #[error("edits overlap at bytes {0}..{1} and {2}..{3}")]
    Overlap(usize, usize, usize, usize),
    #[error("edit {0}..{1} lies outside the text ({2} bytes)")]
    OutOfBounds(usize, usize, usize),
    #[error("edit {0}..{1} does not fall on character boundaries")]
    NotCharBoundary(usize, usize),
}

/// True when two edits touch the same text. Insertions at the boundary of a
/// replacement do not overlap it; two insertions at the same point do not
/// overlap each other.
pub fn overlaps(a: &Edit, b: &Edit) -> bool {
    if a.start == a.end && b.start == b.end {
        return false;
    }
    if a.start == a.end {
        return b.start < a.start && a.start < b.end;
    }
    if b.start == b.end {
        return a.start < b.start && b.start < a.end;
    }
    a.start < b.end && b.start < a.end
}

/// Sort and deduplicate edits, rejecting overlaps.
pub fn normalize(edits: &[Edit]) -> Result<Vec<Edit>, EditError> {
    let mut sorted: Vec<Edit> = edits.to_vec();
    sorted.sort();
    sorted.dedup();
    for pair in sorted.windows(2) {
        if overlaps(&pair[0], &pair[1]) {
            return Err(EditError::Overlap(
                pair[0].start,
                pair[0].end,
                pair[1].start,
                pair[1].end,
            ));
        }
    }
    Ok(sorted)
}

/// Apply a set of non-overlapping edits to `text`.
pub fn apply(text: &str, edits: &[Edit]) -> Result<String, EditError> {
    let sorted = normalize(edits)?;
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for e in &sorted {
        if e.end > text.len() || e.start > e.end {
            return Err(EditError::OutOfBounds(e.start, e.end, text.len()));
        }
        if !text.is_char_boundary(e.start) || !text.is_char_boundary(e.end) {
            return Err(EditError::NotCharBoundary(e.start, e.end));
        }
        out.push_str(&text[cursor..e.start]);
        out.push_str(&e.text);
        cursor = e.end;
    }
    out.push_str(&text[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(start: usize, end: usize, text: &str) -> Edit {
        Edit {
            start,
            end,
            text: text.to_string(),
        }
    }

    #[test]
    fn applies_in_any_order() {
        let out = apply("hello world", &[e(6, 11, "there"), e(0, 5, "hi")]).unwrap();
        assert_eq!(out, "hi there");
    }

    #[test]
    fn insertions_at_boundaries_are_fine() {
        let out = apply("ab", &[e(1, 1, "X"), e(0, 1, "A"), e(1, 2, "B")]).unwrap();
        assert_eq!(out, "AXB");
    }

    #[test]
    fn duplicates_collapse_and_overlaps_fail() {
        assert_eq!(apply("abc", &[e(0, 1, "x"), e(0, 1, "x")]).unwrap(), "xbc");
        assert!(apply("abcdef", &[e(0, 3, ""), e(2, 4, "")]).is_err());
        assert!(apply("abc", &[e(0, 9, "")]).is_err());
    }
}
