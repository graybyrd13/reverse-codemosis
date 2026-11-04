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
