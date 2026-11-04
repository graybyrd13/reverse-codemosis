//! Reading and applying the unified diffs the engine writes, exactly: no
//! fuzz, no offsets. Proof certificates carry their change as a unified
//! diff (the same text a reviewer reads), and verification reverses it to
//! recover the code as it was before the correction.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
    /// (tag, line with its ending): tag is ' ', '-' or '+'.
    pub lines: Vec<(char, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilePatch {
    pub path: String,
    pub hunks: Vec<Hunk>,
}

fn range(s: &str) -> Result<(usize, usize), String> {
    let (start, len) = match s.split_once(',') {
        Some((a, b)) => (a, b),
        None => (s, "1"),
    };
    Ok((
        start.parse().map_err(|_| format!("bad hunk range '{s}'"))?,
        len.parse().map_err(|_| format!("bad hunk range '{s}'"))?,
    ))
}

/// Parse a multi-file unified diff as written by `session::unified_diff`.
pub fn parse(diff: &str) -> Result<Vec<FilePatch>, String> {
    let mut files: Vec<FilePatch> = Vec::new();
    let lines: Vec<&str> = diff.split_inclusive('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(rest) = line.strip_prefix("+++ b/") {
            files.push(FilePatch {
                path: rest.trim_end_matches('\n').to_string(),
                hunks: Vec::new(),
            });
            i += 1;
            continue;
        }
        if let Some(header) = line.strip_prefix("@@ -") {
            let file = files.last_mut().ok_or("a hunk before any file header")?;
            let header = header.trim_end();
            let (old, rest) = header.split_once(" +").ok_or("bad hunk header")?;
            let new = rest.split_once(" @@").map(|(n, _)| n).ok_or("bad hunk header")?;
            let (old_start, old_len) = range(old)?;
            let (new_start, new_len) = range(new)?;
            let mut hunk = Hunk {
                old_start,
                old_len,
                new_start,
                new_len,
                lines: Vec::new(),
            };
            i += 1;
            let (mut seen_old, mut seen_new) = (0, 0);
            while i < lines.len() && (seen_old < old_len || seen_new < new_len) {
                let l = lines[i];
                let mut chars = l.chars();
                let tag = chars.next().ok_or("an empty hunk line")?;
                if tag == '\\' {
                    i += 1;
                    continue;
                }
                if !matches!(tag, ' ' | '-' | '+') {
                    return Err(format!("unexpected line in hunk: {l:?}"));
                }
                let mut content = chars.as_str().to_string();
                // "\ No newline at end of file" after a line means the line has none.
                if lines.get(i + 1).is_some_and(|n| n.starts_with("\\ ")) && content.ends_with('\n') {
                    content.pop();
                }
                match tag {
                    ' ' => {
                        seen_old += 1;
                        seen_new += 1;
                    }
                    '-' => seen_old += 1,
                    _ => seen_new += 1,
                }
                hunk.lines.push((tag, content));
                i += 1;
            }
            file.hunks.push(hunk);
            continue;
        }
        i += 1;
    }
    Ok(files)
}

fn apply_side(text: &str, patch: &FilePatch, reverse: bool) -> Result<String, String> {
    let src: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for h in &patch.hunks {
        // The side we read from and the side we write.
        let (from_start, from_len, keep, drop) = if reverse {
            (h.new_start, h.new_len, '-', '+')
        } else {
            (h.old_start, h.old_len, '+', '-')
        };
        let first = if from_len == 0 { from_start } else { from_start - 1 };
        if first < cursor || first > src.len() {
            return Err(format!(
                "{}: hunk at line {from_start} is out of order or out of range",
                patch.path
            ));
        }
        for l in &src[cursor..first] {
            out.push_str(l);
        }
        cursor = first;
        for (tag, content) in &h.lines {
            if *tag == ' ' || *tag == drop {
                let actual = src.get(cursor).copied().unwrap_or("");
                if actual != content {
                    return Err(format!(
                        "{}: line {} does not match the patch (expected {content:?}, found {actual:?})",
                        patch.path,
                        cursor + 1
                    ));
                }
                cursor += 1;
            }
            if *tag == ' ' || *tag == keep {
                out.push_str(content);
            }
        }
    }
    for l in &src[cursor.min(src.len())..] {
        out.push_str(l);
    }
    Ok(out)
}

/// Apply a file's patch to its old text.
pub fn apply(old: &str, patch: &FilePatch) -> Result<String, String> {
    apply_side(old, patch, false)
}

/// Undo a file's patch on its new text, recovering the old text.
pub fn reverse(new: &str, patch: &FilePatch) -> Result<String, String> {
    apply_side(new, patch, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::unified_diff;

    fn roundtrip(old: &str, new: &str) {
        let d = unified_diff("f.txt", old, new);
        if old == new {
            assert!(d.is_empty());
            return;
        }
        let patches = parse(&d).unwrap();
        assert_eq!(patches.len(), 1, "{d}");
        assert_eq!(
            apply(old, &patches[0]).unwrap_or_else(|e| panic!("{e}\nold={old:?}\nnew={new:?}\n{d}")),
            new,
            "forward:\n{d}"
        );
        assert_eq!(
            reverse(new, &patches[0]).unwrap_or_else(|e| panic!("{e}\nold={old:?}\nnew={new:?}\n{d}")),
            old,
            "reverse:\n{d}"
        );
    }

    #[test]
    fn roundtrips() {
        roundtrip("a\nb\nc\n", "a\nc\n");
        roundtrip("a\nb\nc\n", "a\nb\nc\nd\n");
        roundtrip("a\nb\nc", "a\nB\nc");
        roundtrip("a\nb\nc\n", "a\nb\nc");
        roundtrip("x", "y");
        roundtrip("", "new\n");
        roundtrip("old\n", "");
        let long: String = (0..40).map(|i| format!("line {i}\n")).collect();
        let edited = long.replace("line 3\n", "").replace("line 30\n", "line thirty\n");
        roundtrip(&long, &edited);
    }

    /// A deterministic pseudo-random sweep over many small edits: every diff
    /// the engine writes must reverse exactly.
    #[test]
    fn property_reverse_undoes_every_diff() {
        let mut seed: u64 = 0x9e3779b97f4a7c15;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let alphabet = ["a\n", "b\n", "c\n", "\n", "dd\n", "e"];
        for _ in 0..500 {
            let n = (next() % 12) as usize;
            let old: String = (0..n).map(|_| alphabet[(next() % 6) as usize]).collect();
            let mut new_lines: Vec<&str> = old.split_inclusive('\n').collect();
            for _ in 0..(next() % 4) {
                let op = next() % 3;
                let at = if new_lines.is_empty() {
                    0
                } else {
                    (next() as usize) % (new_lines.len() + 1)
                };
                match op {
                    0 => new_lines.insert(at.min(new_lines.len()), alphabet[(next() % 5) as usize]),
                    1 if !new_lines.is_empty() => {
                        new_lines.remove(at.min(new_lines.len() - 1));
                    }
                    _ if !new_lines.is_empty() => {
                        let i = at.min(new_lines.len() - 1);
                        new_lines[i] = alphabet[(next() % 5) as usize];
                    }
                    _ => {}
                }
            }
            let new: String = new_lines.concat();
            roundtrip(&old, &new);
        }
    }

    /// The diffs must also be accepted by git itself, since reviewers apply them.
    #[test]
    fn git_applies_the_diffs() {
        if std::process::Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let cases = [
            ("b\n\na\nb\nb\nb\n", "\n\na\nb\nb\nb\n"),
            ("a\nb\nc", "a\nB\nc"),
            ("one\n", "one\ntwo\n"),
            ("x\ny\n", "y\n"),
        ];
        for (old, new) in cases {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("f.txt"), old).unwrap();
            std::fs::write(dir.path().join("p.diff"), unified_diff("f.txt", old, new)).unwrap();
            let out = std::process::Command::new("git")
                .args(["apply", "--unsafe-paths", "p.diff"])
                .current_dir(dir.path())
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
            assert_eq!(std::fs::read_to_string(dir.path().join("f.txt")).unwrap(), new);
        }
    }

    #[test]
    fn refuses_a_patch_that_does_not_fit() {
        let d = unified_diff("f.txt", "a\nb\n", "a\nc\n");
        let p = &parse(&d).unwrap()[0];
        assert!(reverse("a\nX\n", p).is_err());
    }
}
