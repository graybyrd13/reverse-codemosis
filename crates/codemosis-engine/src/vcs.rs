//! Version-control safety: corrections are made in place, so by default the
//! engine only works on a clean git working tree, where `git diff` shows
//! exactly what it did and `git checkout .` undoes it.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeState {
    Clean,
    /// Uncommitted changes, with the first few paths.
    Dirty(Vec<String>),
    NotARepository,
    /// git is not installed.
    Unknown,
}

pub fn state(root: &Path) -> TreeState {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["status", "--porcelain", "--untracked-files=normal", "--", "."])
        .output();
    match out {
        Err(_) => TreeState::Unknown,
        Ok(o) if !o.status.success() => TreeState::NotARepository,
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            let paths: Vec<String> = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.get(3..).unwrap_or(l).to_string())
                .collect();
            if paths.is_empty() {
                TreeState::Clean
            } else {
                TreeState::Dirty(paths)
            }
        }
    }
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Files under `root` that differ from the last commit: modified, staged or
/// untracked (and not ignored), relative to `root`. None outside git.
pub fn changed_files(root: &Path) -> Option<Vec<std::path::PathBuf>> {
    let mut paths = std::collections::BTreeSet::new();
    let tracked = git(
        root,
        &["diff", "--name-only", "--relative", "--no-renames", "HEAD", "--", "."],
    )
    // No commit yet: everything staged is new.
    .or_else(|| git(root, &["diff", "--name-only", "--relative", "--cached", "--", "."]))?;
    let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "--", "."])?;
    for line in tracked.lines().chain(untracked.lines()) {
        let line = line.trim();
        if !line.is_empty() && root.join(line).is_file() {
            paths.insert(std::path::PathBuf::from(line));
        }
    }
    Some(paths.into_iter().collect())
}

/// Which lines of a file are new since the last commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedLines {
    /// The whole file is new (untracked, no commit yet, or not in git).
    All,
    /// 1-based inclusive line ranges, in the file as it is now.
    Ranges(Vec<(usize, usize)>),
}

impl ChangedLines {
    /// Whether any line from `first` to `last` (1-based, inclusive) changed.
    pub fn touches(&self, first: usize, last: usize) -> bool {
        match self {
            ChangedLines::All => true,
            ChangedLines::Ranges(r) => r.iter().any(|(a, b)| first <= *b && *a <= last),
        }
    }
}

/// The lines of `rel` (relative to `root`) that differ from the last commit.
pub fn changed_lines(root: &Path, rel: &Path) -> ChangedLines {
    let rel_s = rel.to_string_lossy();
    let tracked = git(root, &["ls-files", "--error-unmatch", "--", &rel_s]).is_some();
    if !tracked {
        return ChangedLines::All;
    }
    let Some(diff) = git(
        root,
        &[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--unified=0",
            "HEAD",
            "--",
            &rel_s,
        ],
    ) else {
        return ChangedLines::All;
    };
    ChangedLines::Ranges(hunk_ranges(&diff))
}

/// New-side line ranges of a unified diff with no context.
fn hunk_ranges(diff: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for line in diff.lines().filter(|l| l.starts_with("@@ ")) {
        let Some(new) = line.split_whitespace().find(|w| w.starts_with('+')) else {
            continue;
        };
        let mut parts = new[1..].splitn(2, ',');
        let start: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let count: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
        if count == 0 {
            // Pure deletion: the lines around it are where the code changed.
            out.push((start.max(1), start + 1));
        } else {
            out.push((start, start + count - 1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunks_give_new_side_ranges() {
        let diff = "diff --git a/x b/x\n@@ -3 +3,2 @@\n-a\n+b\n+c\n@@ -10,2 +11,0 @@\n-d\n-e\n@@ -20 +19 @@\n-f\n+g\n";
        assert_eq!(hunk_ranges(diff), vec![(3, 4), (11, 12), (19, 19)]);
        let c = ChangedLines::Ranges(hunk_ranges(diff));
        assert!(c.touches(4, 6) && c.touches(19, 19) && !c.touches(5, 10));
        assert!(ChangedLines::All.touches(1, 1));
    }

    #[test]
    fn changes_against_the_last_commit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            assert!(ok, "git {args:?}");
        };
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        run(&["init", "-q"]);
        std::fs::write(root.join("a.py"), "x = 1\ny = 2\nz = 3\n").unwrap();
        std::fs::write(root.join("b.py"), "b = 1\n").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "init",
        ]);
        std::fs::write(root.join("a.py"), "x = 1\ny = 20\nz = 3\n").unwrap();
        std::fs::write(root.join("new.py"), "n = 1\n").unwrap();
        let changed = changed_files(root).unwrap();
        assert_eq!(
            changed,
            vec![std::path::PathBuf::from("a.py"), std::path::PathBuf::from("new.py")]
        );
        assert_eq!(
            changed_lines(root, Path::new("a.py")),
            ChangedLines::Ranges(vec![(2, 2)])
        );
        assert_eq!(changed_lines(root, Path::new("new.py")), ChangedLines::All);
        assert_eq!(changed_lines(root, Path::new("b.py")), ChangedLines::Ranges(vec![]));
    }
}
