//! Every relative link in the documentation points at a file that exists.

use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !matches!(name.as_str(), "node_modules" | "target" | ".git" | "out") {
                markdown_files(&p, out);
            }
        } else if name.ends_with(".md") {
            out.push(p);
        }
    }
}

/// The targets of `[text](target)` links, outside code blocks.
fn links(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let mut rest = line;
        while let Some(i) = rest.find("](") {
            let after = &rest[i + 2..];
            let Some(end) = after.find(')') else { break };
            out.push(after[..end].trim().to_string());
            rest = &after[end..];
        }
    }
    out
}

#[test]
fn relative_links_resolve() {
    let root = workspace();
    let mut files = Vec::new();
    for top in [
        "README.md",
        "CONTRIBUTING.md",
        "AGENTS.md",
        "SECURITY.md",
        "GOVERNANCE.md",
        "CHANGELOG.md",
    ] {
        let p = root.join(top);
        if p.is_file() {
            files.push(p);
        }
    }
    for dir in ["docs", "integrations", "editors", "examples", "rfcs"] {
        markdown_files(&root.join(dir), &mut files);
    }
    let mut broken = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        for target in links(&text) {
            if target.is_empty()
                || target.starts_with('#')
                || target.contains("://")
                || target.starts_with("mailto:")
                || target.contains(' ')
            {
                continue;
            }
            let path = target.split('#').next().unwrap();
            if !file.parent().unwrap().join(path).exists() {
                broken.push(format!("{}: {target}", file.strip_prefix(&root).unwrap().display()));
            }
        }
    }
    assert!(broken.is_empty(), "broken links:\n{}", broken.join("\n"));
    assert!(files.len() > 20, "found only {} files", files.len());
}
