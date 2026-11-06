//! `codemosis integrate <tool>`: connect Reverse Codemosis to an agentic
//! coding tool or editor by writing that tool's project configuration.
//!
//! JSON files are merged, never replaced: existing settings stay, and an
//! entry that is already there is left alone. A file that cannot be read as
//! JSON (for example one with comments) is not touched; the snippet to add is
//! printed instead. Tools configured only per user (Codex, Windsurf, Cline)
//! get instructions, not edits to files outside the project.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::Path;

/// The tools `integrate` knows, with a one-line description.
pub const TOOLS: &[(&str, &str)] = &[
    (
        "claude-code",
        "Claude Code: MCP server (.mcp.json) and a hook after every edit (.claude/settings.json)",
    ),
    (
        "cursor",
        "Cursor: MCP server (.cursor/mcp.json) and a project rule (.cursor/rules/codemosis.mdc)",
    ),
    (
        "vscode",
        "VS Code / GitHub Copilot agent mode: MCP server (.vscode/mcp.json)",
    ),
    ("gemini", "Gemini CLI: MCP server (.gemini/settings.json)"),
    ("zed", "Zed: MCP context server (.zed/settings.json)"),
    ("continue", "Continue: MCP server (.continue/mcpServers/codemosis.yaml)"),
    ("opencode", "opencode: MCP server (opencode.json)"),
    (
        "aider",
        "Aider: Reverse Codemosis as the lint command (.aider.conf.yml)",
    ),
    (
        "agents-md",
        "AGENTS.md: how any agent should use Reverse Codemosis (Codex, Jules, Amp, ...)",
    ),
    ("codex", "OpenAI Codex CLI: prints the ~/.codex/config.toml entry"),
    ("windsurf", "Windsurf: prints the mcp_config.json entry"),
    ("cline", "Cline: prints the MCP settings entry"),
];

/// What an agent should know, for AGENTS.md, Cursor rules and the like.
pub const AGENT_GUIDE: &str = "\
## Reverse Codemosis

This project uses [Reverse Codemosis](https://github.com/graybyrd13/reverse-codemosis), which finds where code
falls short and corrects only what it can prove.

- After changing code, check it: `codemosis check --changed --format compact` (or the `check` tool of the
  `codemosis` MCP server, with the files you changed).
- A finding marked `proven correction` is safe to apply: make exactly the change it shows, or run
  `codemosis fix --allow-dirty` (each step is verified with the build and tests).
- A finding with a `question` is the user's decision (for example, keep or remove a fixed wait). Ask the user,
  then record their answer with `codemosis answer <id> <answer>`. Never answer for them.
- An `explained` finding has no provable correction: mention the risk to the user rather than guessing a fix.
- Do not add `codemosis: ignore[...]` comments unless the user asks for them, with their reason.
";

enum Change {
    /// Set `value` at `pointer` in a JSON file, unless something is there.
    JsonEntry {
        file: &'static str,
        pointer: &'static [&'static str],
        value: Value,
    },
    /// Append `entry` to the array at `pointer`, unless an element mentions `marker`.
    JsonAppend {
        file: &'static str,
        pointer: &'static [&'static str],
        entry: Value,
        marker: &'static str,
    },
    /// Write a whole file, unless it exists.
    NewFile { file: &'static str, content: String },
    /// Append a section to a text file, unless it already mentions `marker`.
    AppendText {
        file: &'static str,
        content: String,
        marker: &'static str,
    },
}

fn mcp_entry() -> Value {
    json!({"command": "codemosis", "args": ["mcp"]})
}

fn changes(tool: &str) -> Option<Vec<Change>> {
    Some(match tool {
        "claude-code" => vec![
            Change::JsonEntry {
                file: ".mcp.json",
                pointer: &["mcpServers", "codemosis"],
                value: mcp_entry(),
            },
            Change::JsonAppend {
                file: ".claude/settings.json",
                pointer: &["hooks", "PostToolUse"],
                entry: json!({
                    "matcher": "Edit|MultiEdit|Write",
                    "hooks": [{"type": "command", "command": "codemosis hook", "timeout": 120}]
                }),
                marker: "codemosis hook",
            },
        ],
        "cursor" => vec![
            Change::JsonEntry {
                file: ".cursor/mcp.json",
                pointer: &["mcpServers", "codemosis"],
                value: mcp_entry(),
            },
            Change::NewFile {
                file: ".cursor/rules/codemosis.mdc",
                content: format!(
                    "---\ndescription: Check and correct code with Reverse Codemosis\nalwaysApply: true\n---\n\n{}",
                    AGENT_GUIDE.trim_start_matches("## Reverse Codemosis\n\n")
                ),
            },
        ],
        "vscode" => vec![Change::JsonEntry {
            file: ".vscode/mcp.json",
            pointer: &["servers", "codemosis"],
            value: json!({"type": "stdio", "command": "codemosis", "args": ["mcp"]}),
        }],
        "gemini" => vec![Change::JsonEntry {
            file: ".gemini/settings.json",
            pointer: &["mcpServers", "codemosis"],
            value: mcp_entry(),
        }],
        "zed" => vec![Change::JsonEntry {
            file: ".zed/settings.json",
            pointer: &["context_servers", "codemosis"],
            value: json!({"source": "custom", "command": "codemosis", "args": ["mcp"]}),
        }],
        "continue" => vec![Change::NewFile {
            file: ".continue/mcpServers/codemosis.yaml",
            content: "name: Reverse Codemosis\nversion: 0.0.1\nschema: v1\nmcpServers:\n  - name: codemosis\n    command: codemosis\n    args:\n      - mcp\n".into(),
        }],
        "opencode" => vec![Change::JsonEntry {
            file: "opencode.json",
            pointer: &["mcp", "codemosis"],
            value: json!({"type": "local", "command": ["codemosis", "mcp"], "enabled": true}),
        }],
        "aider" => vec![Change::AppendText {
            file: ".aider.conf.yml",
            content: "# Reverse Codemosis checks every file Aider edits; Aider offers to fix what it reports.\nlint-cmd:\n  - \"codemosis check --format compact --fail-on any --file\"\nauto-lint: true\n".into(),
            marker: "codemosis check",
        }],
        "agents-md" => vec![Change::AppendText {
            file: "AGENTS.md",
            content: AGENT_GUIDE.to_string(),
            marker: "## Reverse Codemosis",
        }],
        _ => return None,
    })
}

/// Instructions for tools configured per user, outside the project.
fn instructions(tool: &str) -> Option<&'static str> {
    Some(match tool {
        "codex" => "Add to ~/.codex/config.toml:\n\n[mcp_servers.codemosis]\ncommand = \"codemosis\"\nargs = [\"mcp\"]\n\nCodex also reads AGENTS.md: `codemosis integrate agents-md` writes guidance there.",
        "windsurf" => "Add to ~/.codeium/windsurf/mcp_config.json:\n\n{\n  \"mcpServers\": {\n    \"codemosis\": { \"command\": \"codemosis\", \"args\": [\"mcp\"] }\n  }\n}",
        "cline" => "In Cline, open MCP Servers > Configure and add to cline_mcp_settings.json:\n\n{\n  \"mcpServers\": {\n    \"codemosis\": { \"command\": \"codemosis\", \"args\": [\"mcp\"] }\n  }\n}",
        _ => return None,
    })
}

fn pointer_get<'v>(v: &'v Value, pointer: &[&str]) -> Option<&'v Value> {
    pointer.iter().try_fold(v, |cur, k| cur.get(k))
}

/// The object at `pointer`, creating objects on the way.
fn pointer_parent<'v>(v: &'v mut Value, pointer: &[&str]) -> Result<&'v mut serde_json::Map<String, Value>> {
    let mut cur = v;
    for k in pointer {
        let obj = cur
            .as_object_mut()
            .with_context(|| format!("`{k}`'s parent is not a JSON object"))?;
        cur = obj.entry(k.to_string()).or_insert_with(|| json!({}));
    }
    cur.as_object_mut().context("not a JSON object")
}

fn snippet(pointer: &[&str], value: &Value) -> String {
    let mut v = value.clone();
    for k in pointer.iter().rev() {
        v = json!({ *k: v });
    }
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// Apply one change under `root`. Returns a line describing what happened.
fn apply(root: &Path, change: &Change) -> Result<String> {
    match change {
        Change::JsonEntry { file, pointer, value }
        | Change::JsonAppend {
            file,
            pointer,
            entry: value,
            ..
        } => {
            let path = root.join(file);
            let mut doc: Value = if path.is_file() {
                let text = std::fs::read_to_string(&path)?;
                match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => {
                        return Ok(format!(
                            "! {file} is not plain JSON (comments?), so it was left alone; add this yourself:\n{}",
                            snippet(pointer, value)
                        ))
                    }
                }
            } else {
                json!({})
            };
            match change {
                Change::JsonEntry { .. } => {
                    if pointer_get(&doc, pointer).is_some() {
                        return Ok(format!("= {file}: `{}` is already configured", pointer.join(".")));
                    }
                    let (last, parent) = pointer.split_last().expect("non-empty pointer");
                    pointer_parent(&mut doc, parent)?.insert(last.to_string(), value.clone());
                }
                Change::JsonAppend { marker, .. } => {
                    if pointer_get(&doc, pointer).is_some_and(|v| v.to_string().contains(marker)) {
                        return Ok(format!("= {file}: already runs `{marker}`"));
                    }
                    let (last, parent) = pointer.split_last().expect("non-empty pointer");
                    let obj = pointer_parent(&mut doc, parent)?;
                    let list = obj.entry(last.to_string()).or_insert_with(|| json!([]));
                    match list.as_array_mut() {
                        Some(a) => a.push(value.clone()),
                        None => bail!("{file}: `{}` is not a list", pointer.join(".")),
                    }
                }
                _ => unreachable!(),
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, serde_json::to_string_pretty(&doc)? + "\n")
                .with_context(|| format!("cannot write {}", path.display()))?;
            Ok(format!("+ {file}"))
        }
        Change::NewFile { file, content } => {
            let path = root.join(file);
            if path.exists() {
                return Ok(format!("= {file} exists; left alone"));
            }
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, content).with_context(|| format!("cannot write {}", path.display()))?;
            Ok(format!("+ {file}"))
        }
        Change::AppendText { file, content, marker } => {
            let path = root.join(file);
            let existing = std::fs::read_to_string(&path).unwrap_or_default();
            if existing.contains(marker) {
                return Ok(format!("= {file} already mentions Reverse Codemosis"));
            }
            let sep = if existing.is_empty() || existing.ends_with("\n\n") {
                ""
            } else if existing.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            std::fs::write(&path, format!("{existing}{sep}{content}"))
                .with_context(|| format!("cannot write {}", path.display()))?;
            Ok(format!("+ {file}"))
        }
    }
}

/// Run `codemosis integrate`. Prints what it did.
pub fn integrate(root: &Path, tool: Option<&str>) -> Result<()> {
    let Some(tool) = tool else {
        println!("Connect Reverse Codemosis to a tool: codemosis integrate <tool>\n");
        for (name, what) in TOOLS {
            println!("  {name:<12} {what}");
        }
        return Ok(());
    };
    if let Some(text) = instructions(tool) {
        println!("{text}");
        return Ok(());
    }
    let Some(list) = changes(tool) else {
        bail!(
            "unknown tool '{tool}'; known: {}",
            TOOLS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
        );
    };
    for c in &list {
        println!("{}", apply(root, c)?);
    }
    println!("\n`codemosis` must be on PATH for the tool to start it (`cargo install --locked --path crates/codemosis-cli`).");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_and_never_clobbers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        std::fs::write(
            root.join(".claude/settings.json"),
            r#"{"permissions": {"allow": ["Bash(ls)"]}, "hooks": {"PostToolUse": [{"matcher": "Write", "hooks": []}]}}"#,
        )
        .unwrap();
        std::fs::write(root.join(".mcp.json"), r#"{"mcpServers": {"other": {"command": "x"}}}"#).unwrap();
        integrate(root, Some("claude-code")).unwrap();
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join(".claude/settings.json")).unwrap()).unwrap();
        assert_eq!(settings["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(settings["hooks"]["PostToolUse"].as_array().unwrap().len(), 2);
        assert_eq!(
            settings["hooks"]["PostToolUse"][1]["hooks"][0]["command"],
            "codemosis hook"
        );
        let mcp: Value = serde_json::from_str(&std::fs::read_to_string(root.join(".mcp.json")).unwrap()).unwrap();
        assert_eq!(mcp["mcpServers"]["other"]["command"], "x");
        assert_eq!(mcp["mcpServers"]["codemosis"]["args"][0], "mcp");

        // Running it again changes nothing.
        let before = std::fs::read_to_string(root.join(".claude/settings.json")).unwrap();
        integrate(root, Some("claude-code")).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join(".claude/settings.json")).unwrap(),
            before
        );
    }

    #[test]
    fn files_with_comments_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".vscode")).unwrap();
        let jsonc = "{\n  // my servers\n  \"servers\": {}\n}\n";
        std::fs::write(root.join(".vscode/mcp.json"), jsonc).unwrap();
        let line = apply(root, &changes("vscode").unwrap()[0]).unwrap();
        assert!(line.starts_with("! .vscode/mcp.json is not plain JSON"), "{line}");
        assert!(line.contains("\"codemosis\""));
        assert_eq!(std::fs::read_to_string(root.join(".vscode/mcp.json")).unwrap(), jsonc);
    }

    #[test]
    fn agents_md_is_appended_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("AGENTS.md"), "# Project\n\nRun `make test`.\n").unwrap();
        integrate(root, Some("agents-md")).unwrap();
        integrate(root, Some("agents-md")).unwrap();
        let text = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        assert!(text.starts_with("# Project\n\nRun `make test`.\n\n## Reverse Codemosis"));
        assert_eq!(text.matches("## Reverse Codemosis").count(), 1);
        assert!(text.contains("Never answer for them"));
    }

    #[test]
    fn every_tool_is_known() {
        for (name, _) in TOOLS {
            assert!(changes(name).is_some() || instructions(name).is_some(), "{name}");
        }
        assert!(integrate(Path::new("."), Some("no-such-tool")).is_err());
    }
}
