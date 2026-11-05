//! What agentic coding tools need from the command line: checks scoped to
//! the files (and lines) an agent just changed, findings one per line, and
//! `codemosis hook`, which reads a tool's hook payload from standard input
//! and tells the agent what it should do about each finding.

use anyhow::Result;
use codemosis_core::{Finding, Project, Status};
use codemosis_engine::policy::Policy;
use codemosis_engine::vcs::{self, ChangedLines};
use codemosis_engine::Workspace;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// A path as given on the command line or by a tool, relative to `root`.
pub fn relative_to(root: &Path, path: &Path) -> Option<PathBuf> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let abs = abs.canonicalize().unwrap_or(abs);
    if let Ok(rel) = abs.strip_prefix(root) {
        return Some(rel.to_path_buf());
    }
    // Relative to the project root rather than the working directory.
    (!path.is_absolute() && root.join(path).exists()).then(|| path.to_path_buf())
}

/// The project files to scan: those named, plus (with `changed`) every file
/// that differs from the last commit. None means the whole project.
pub fn scope(root: &Path, project: &Project, files: &[PathBuf], changed: bool) -> Result<Option<BTreeSet<usize>>> {
    if files.is_empty() && !changed {
        return Ok(None);
    }
    let mut wanted: Vec<PathBuf> = files.iter().filter_map(|f| relative_to(root, f)).collect();
    if changed {
        match vcs::changed_files(root) {
            Some(c) => wanted.extend(c),
            None => anyhow::bail!("--changed needs a git repository at {}", root.display()),
        }
    }
    let mut set = BTreeSet::new();
    for rel in wanted {
        match project.index_of(&rel) {
            Some(i) => {
                set.insert(i);
            }
            None => codemosis_core::debug!(
                "{} is not part of the project (ignored, generated, vendored or not text)",
                rel.display()
            ),
        }
    }
    Ok(Some(set))
}

fn status_note(f: &Finding) -> String {
    match &f.status {
        Status::Fixable => "proven correction".to_string(),
        Status::NeedsAnswer { question } => {
            let options: Vec<&str> = question.options.iter().map(|o| o.answer.as_str()).collect();
            format!("question {}: {}", question.id, options.join(" | "))
        }
        Status::Declined { answer, .. } => format!("declined ({answer})"),
        Status::Explained => "explained".to_string(),
        Status::Refused { reason } => format!("refused: {reason}"),
        Status::Suppressed { .. } => "suppressed".to_string(),
    }
}

/// `path:line:col: level: message [rule; status]`: one line per finding, for
/// hooks, editors' quickfix lists and grep.
pub fn compact_line(f: &Finding, policy: &Policy) -> String {
    format!(
        "{}:{}:{}: {}: {} [{}; {}]",
        f.file.display(),
        f.span.start.line,
        f.span.start.column,
        policy.level(&f.rule, f.category).as_str(),
        f.message.lines().next().unwrap_or(""),
        f.rule,
        status_note(f)
    )
}

/// The changed lines of a finding's correction: `-`/`+` lines only.
fn correction_lines(project: &Project, f: &Finding) -> Vec<String> {
    let Some(file) = project.index_of(&f.file).map(|i| &project.files[i]) else {
        return Vec::new();
    };
    let Ok(new) = codemosis_core::edit::apply(&file.text, &f.edits) else {
        return Vec::new();
    };
    let diff = codemosis_engine::session::unified_diff(&file.display_path(), &file.text, &new);
    let diff = codemosis_core::redact::redact(&diff).into_owned();
    diff.lines()
        .filter(|l| (l.starts_with('-') || l.starts_with('+')) && !l.starts_with("---") && !l.starts_with("+++"))
        .map(String::from)
        .collect()
}

/// Findings worth an agent's attention: open, and not merely refused.
pub fn actionable(f: &Finding) -> bool {
    matches!(
        f.status,
        Status::Fixable | Status::NeedsAnswer { .. } | Status::Explained
    )
}

/// A report written for an agent: what was found and what to do about it.
pub fn agent_report(project: &Project, findings: &[&Finding], scope_note: &str) -> String {
    let mut out = format!(
        "Reverse Codemosis found {} issue{} in {scope_note}.\n",
        findings.len(),
        if findings.len() == 1 { "" } else { "s" }
    );
    for f in findings {
        let _ = writeln!(
            out,
            "\n{}:{}:{} {} ({})",
            f.file.display(),
            f.span.start.line,
            f.span.start.column,
            f.rule,
            match &f.status {
                Status::Fixable => format!(
                    "proven correction{}",
                    f.semantics.map(|s| format!(", {}", s.as_str())).unwrap_or_default()
                ),
                Status::NeedsAnswer { .. } => "a decision for the user".to_string(),
                _ => "explained: needs judgment".to_string(),
            }
        );
        let _ = writeln!(out, "  {}", f.message.lines().next().unwrap_or(""));
        match &f.status {
            Status::Fixable => {
                let lines = correction_lines(project, f);
                if !lines.is_empty() {
                    out.push_str("  The correction:\n");
                    for l in lines.iter().take(12) {
                        let _ = writeln!(out, "    {l}");
                    }
                    if lines.len() > 12 {
                        let _ = writeln!(out, "    ... ({} more lines)", lines.len() - 12);
                    }
                }
                let _ = writeln!(
                    out,
                    "  Apply exactly this change, or run: codemosis fix --rule {} --allow-dirty",
                    f.rule
                );
            }
            Status::NeedsAnswer { question } => {
                let _ = writeln!(out, "  Ask the user: {}", question.text);
                for o in &question.options {
                    let _ = writeln!(out, "    - {}: {}", o.answer, o.means);
                }
                let _ = writeln!(
                    out,
                    "  Then record their answer: codemosis answer {} <answer>. Do not choose for them.",
                    question.id
                );
            }
            _ => {
                let _ = writeln!(
                    out,
                    "  No correction can be proven here. Explain the risk to the user; `codemosis explain {}` has the details.",
                    f.rule
                );
            }
        }
    }
    out
}

/// Every file path mentioned in a hook payload, under the keys tools use.
fn payload_paths(v: &serde_json::Value, out: &mut Vec<PathBuf>) {
    const KEYS: &[&str] = &[
        "file_path",
        "filePath",
        "path",
        "notebook_path",
        "target_file",
        "targetFile",
    ];
    const LISTS: &[&str] = &["file_paths", "filePaths", "paths", "files"];
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                match val {
                    serde_json::Value::String(s) if KEYS.contains(&k.as_str()) && !s.is_empty() => {
                        out.push(PathBuf::from(s));
                    }
                    serde_json::Value::Array(items) if LISTS.contains(&k.as_str()) => {
                        out.extend(items.iter().filter_map(|i| i.as_str()).map(PathBuf::from));
                    }
                    // Tool output is not what the agent changed.
                    _ if k == "tool_response" || k == "tool_output" => {}
                    _ => payload_paths(val, out),
                }
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|i| payload_paths(i, out)),
        _ => {}
    }
}

/// The project root for a directory: the nearest ancestor with a
/// `codemosis.toml`, else the git work tree, else the directory itself.
fn project_root(dir: &Path) -> PathBuf {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if let Some(d) = dir
        .ancestors()
        .find(|d| d.join(codemosis_engine::config::FILE_NAME).is_file())
    {
        return d.to_path_buf();
    }
    std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or(dir)
}

pub struct HookOptions {
    pub path: Option<PathBuf>,
    pub all_lines: bool,
    pub exit_code: u8,
    pub json: bool,
}

/// `codemosis hook`: check what an agent just changed and report back.
///
/// Reads the tool's hook payload (JSON) from standard input, if any. The
/// files it names are checked; with none (a "stop" or "after turn" hook),
/// every file changed since the last commit is. Only findings on lines that
/// differ from the last commit are reported, unless `all_lines`. With
/// findings, the report goes to standard error and the exit status is
/// `exit_code` (2 by default, which Claude Code and Gemini CLI feed back to
/// the model); without, nothing is printed and the status is 0.
pub fn hook(o: HookOptions) -> Result<std::process::ExitCode> {
    use std::io::Read;
    let mut input = String::new();
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        std::io::stdin().read_to_string(&mut input)?;
    }
    let payload: serde_json::Value = serde_json::from_str(input.trim()).unwrap_or(serde_json::Value::Null);
    // Claude Code sets this when a stop hook already made the agent go on:
    // reporting again could loop forever on a question only the user can answer.
    if payload.get("stop_hook_active").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(std::process::ExitCode::SUCCESS);
    }
    let dir = o
        .path
        .clone()
        .or_else(|| payload.get("cwd").and_then(|c| c.as_str()).map(PathBuf::from))
        .unwrap_or(std::env::current_dir()?);
    let root = project_root(&dir);
    let mut named = Vec::new();
    payload_paths(&payload, &mut named);
    let named: Vec<PathBuf> = named
        .into_iter()
        .map(|p| if p.is_absolute() { p } else { dir.join(p) })
        .collect();

    let ws = Workspace::open(&root, &[])?;
    let files = scope(&root, &ws.project, &named, named.is_empty())
        .or_else(|_| scope(&root, &ws.project, &named, false))?
        .unwrap_or_default();
    if files.is_empty() {
        return Ok(std::process::ExitCode::SUCCESS);
    }
    let scan = ws.scan_files(&[], &files);
    let baseline = ws
        .config
        .policy
        .baseline
        .as_ref()
        .and_then(|b| codemosis_engine::policy::Baseline::load(&root.join(b)).ok());
    let mut changed: std::collections::HashMap<PathBuf, ChangedLines> = Default::default();
    let findings: Vec<&Finding> = scan
        .findings
        .iter()
        .filter(|f| actionable(f))
        .filter(|f| baseline.as_ref().is_none_or(|b| !b.contains(f)))
        .filter(|f| {
            o.all_lines
                || changed
                    .entry(f.file.clone())
                    .or_insert_with(|| vcs::changed_lines(&root, &f.file))
                    .touches(f.span.start.line, f.span.end.line)
        })
        .collect();
    if findings.is_empty() {
        return Ok(std::process::ExitCode::SUCCESS);
    }
    if o.json {
        let v = serde_json::json!({
            "schema": "codemosis/hook/v1",
            "project": root.display().to_string(),
            "findings": findings.iter().map(|f| {
                let mut v = serde_json::to_value(f).unwrap_or_default();
                v["fingerprint"] = serde_json::json!(f.fingerprint());
                v["correction"] = serde_json::json!(correction_lines(&ws.project, f));
                v
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(std::process::ExitCode::SUCCESS);
    }
    let note = if o.all_lines {
        "the files you changed"
    } else {
        "the code you just changed"
    };
    eprint!("{}", agent_report(&ws.project, &findings, note));
    Ok(std::process::ExitCode::from(o.exit_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_from_hook_payloads() {
        let claude = serde_json::json!({
            "session_id": "s", "cwd": "/repo", "hook_event_name": "PostToolUse", "tool_name": "Edit",
            "tool_input": {"file_path": "/repo/src/a.py", "old_string": "x", "new_string": "y"},
            "tool_response": {"filePath": "/repo/src/ignored.py"}
        });
        let mut p = Vec::new();
        payload_paths(&claude, &mut p);
        assert_eq!(p, vec![PathBuf::from("/repo/src/a.py")]);

        let cursor = serde_json::json!({"hook_event_name": "afterFileEdit", "file_path": "src/b.ts", "edits": []});
        let mut p = Vec::new();
        payload_paths(&cursor, &mut p);
        assert_eq!(p, vec![PathBuf::from("src/b.ts")]);

        let many = serde_json::json!({"tool_input": {"paths": ["a.go", "b.go"]}});
        let mut p = Vec::new();
        payload_paths(&many, &mut p);
        assert_eq!(p.len(), 2);
    }
}
