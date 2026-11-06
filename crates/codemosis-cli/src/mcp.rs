//! `codemosis mcp`: a Model Context Protocol server over standard input and
//! output, so agentic coding tools (Claude Code, Cursor, Codex, Copilot,
//! Windsurf, Zed, Gemini CLI...) can check code, apply proven corrections and
//! put the questions to their user.
//!
//! JSON-RPC 2.0, one message per line. Standard output carries only protocol
//! messages; logs go to standard error. Tools report their own failures as
//! results with `isError`, as the protocol asks, so the model can read them.
//!
//! The questions belong to the user. The `answer` tool records an answer the
//! user gave; its description tells the model never to choose for them.

use crate::agent;
use anyhow::{anyhow, bail, Result};
use codemosis_core::{Finding, Status};
use codemosis_engine::report::Style;
use codemosis_engine::{oracle_from, vcs, Workspace};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Protocol versions this server speaks, newest first.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Reverse Codemosis finds where code falls short and corrects only what it can prove. \
After editing code, call `check` with the files you changed. A finding with status `fixable` has a proven \
correction: apply it with `fix` (preview first, then apply=true), or make exactly the change shown. A finding \
with a question is the user's decision: ask the user, then record their answer with `answer`; never answer on \
their behalf. An `explained` finding has no provable correction: tell the user about the risk.";

const PLAIN: Style = Style { color: false };

/// The tools, as `tools/list` describes them.
pub fn tools() -> Value {
    let path = json!({"type": "string", "description": "Project directory, absolute or relative to the server's project. Default: the server's project."});
    let rules = json!({"type": "array", "items": {"type": "string"}, "description": "Only these rule or pack ids, e.g. \"python-core\" or \"python-core/is-literal\"."});
    json!([
        {
            "name": "check",
            "title": "Check code",
            "description": "Find where the code falls short: proven corrections, questions for the user, and risks that need judgment. Changes nothing. Pass the files you edited (or changed=true for every file changed since the last commit) to keep the report focused.",
            "inputSchema": {"type": "object", "properties": {
                "path": path,
                "files": {"type": "array", "items": {"type": "string"}, "description": "Only report findings in these files."},
                "changed": {"type": "boolean", "description": "Only report findings in files changed since the last commit."},
                "changed_lines_only": {"type": "boolean", "description": "Only report findings on lines changed since the last commit (default false)."},
                "rules": rules,
            }},
            "annotations": {"title": "Check code", "readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "fix",
            "title": "Apply proven corrections",
            "description": "Apply the corrections that can be proven, one verified step at a time: each step is kept only if every target is gone, nothing worse appears, every file still parses, formatting holds and (with verify) the project's build and tests do not regress. apply=false (the default) previews the diff and changes nothing. apply=true writes the files and a proof certificate; it needs a clean git tree unless allow_dirty=true. Verification runs the project's own build and tests, which can take minutes.",
            "inputSchema": {"type": "object", "properties": {
                "path": path,
                "apply": {"type": "boolean", "description": "Write the corrections (default false: preview only)."},
                "verify": {"type": "boolean", "description": "Run the project's build and tests after each step (default true; only with apply)."},
                "allow_dirty": {"type": "boolean", "description": "Allow uncommitted changes in the working tree (default false)."},
                "rules": rules,
            }},
            "annotations": {"title": "Apply proven corrections", "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false}
        },
        {
            "name": "answer",
            "title": "Record the user's answer",
            "description": "Record the user's answer to one of the questions `check` reported (for example keep or remove a fixed wait). Only call this with an answer the user actually gave you: these are the user's decisions, never yours. The answer is stored in .codemosis/answers.toml for review; `fix` then applies what it asks for.",
            "inputSchema": {"type": "object", "properties": {
                "question_id": {"type": "string", "description": "The question's id, such as Q-0e614498."},
                "answer": {"type": "string", "description": "One of the question's answers, as the user chose it."},
                "note": {"type": "string", "description": "Why, in the user's words (optional)."},
                "path": path,
            }, "required": ["question_id", "answer"]},
            "annotations": {"title": "Record the user's answer", "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
        },
        {
            "name": "explain",
            "title": "Explain a rule",
            "description": "Explain a rule: what it finds, why it matters, the facts that must hold before it corrects anything, its question and its proof.",
            "inputSchema": {"type": "object", "properties": {
                "rule": {"type": "string", "description": "A rule id, such as python-core/is-literal."},
            }, "required": ["rule"]},
            "annotations": {"title": "Explain a rule", "readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "map",
            "title": "Map the project",
            "description": "What Reverse Codemosis understands about the project: its languages and how deeply each is understood, its build and test commands, its formatters and its definitions.",
            "inputSchema": {"type": "object", "properties": {"path": path}},
            "annotations": {"title": "Map the project", "readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "rules",
            "title": "List rules",
            "description": "List the available rules, optionally only those whose id, title or language contains a text.",
            "inputSchema": {"type": "object", "properties": {
                "query": {"type": "string", "description": "Only rules whose id, title or language contains this text."},
            }},
            "annotations": {"title": "List rules", "readOnlyHint": true, "openWorldHint": false}
        },
        {
            "name": "verify_proof",
            "title": "Verify a proof certificate",
            "description": "Re-derive a recorded change from its proof certificate (.codemosis/proofs/*.json): recover the original code and replay every step with the pinned rules, checking each reproduces exactly.",
            "inputSchema": {"type": "object", "properties": {
                "certificate": {"type": "string", "description": "The certificate file, relative to the project."},
                "path": path,
            }, "required": ["certificate"]},
            "annotations": {"title": "Verify a proof certificate", "readOnlyHint": true, "openWorldHint": false}
        }
    ])
}

struct Server {
    root: PathBuf,
}

/// What a tool produced: text for the model, and optional structure.
struct Output {
    text: String,
    structured: Option<Value>,
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

fn arg_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn arg_list(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|i| i.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// How many findings a reply lists in full.
const MAX_LISTED: usize = 50;

fn finding_value(ws: &Workspace, f: &Finding) -> Value {
    let mut v = json!({
        "file": f.file.to_string_lossy().replace('\\', "/"),
        "line": f.span.start.line,
        "column": f.span.start.column,
        "end_line": f.span.end.line,
        "rule": f.rule,
        "title": f.title,
        "category": f.category.as_str(),
        "level": ws.policy.level(&f.rule, f.category).as_str(),
        "message": f.message,
        "fingerprint": f.fingerprint(),
    });
    v["status"] = match &f.status {
        Status::Fixable => json!("fixable"),
        Status::NeedsAnswer { .. } => json!("question"),
        Status::Declined { .. } => json!("declined"),
        Status::Explained => json!("explained"),
        Status::Refused { .. } => json!("refused"),
        Status::Suppressed { .. } => json!("suppressed"),
    };
    if let Some(q) = f.question().filter(|_| matches!(f.status, Status::NeedsAnswer { .. })) {
        v["question"] = json!({
            "id": q.id,
            "text": q.text,
            "options": q.options.iter().map(|o| json!({"answer": o.answer, "means": o.means, "changes_code": o.changes})).collect::<Vec<_>>(),
        });
    }
    if let Status::Refused { reason } = &f.status {
        v["reason"] = json!(reason);
    }
    if let Some(s) = f.semantics.filter(|_| f.is_fixable()) {
        v["semantics"] = json!(s.as_str());
    }
    v
}

impl Server {
    fn project(&self, args: &Value) -> Result<PathBuf> {
        let p = match arg_str(args, "path") {
            None => self.root.clone(),
            Some(p) if Path::new(p).is_absolute() => PathBuf::from(p),
            Some(p) => self.root.join(p),
        };
        p.canonicalize()
            .map_err(|e| anyhow!("cannot open {}: {e}", p.display()))
    }

    fn check(&self, args: &Value) -> Result<Output> {
        let root = self.project(args)?;
        let ws = Workspace::open(&root, &[])?;
        let rules = arg_list(args, "rules");
        let files: Vec<PathBuf> = arg_list(args, "files")
            .into_iter()
            .map(|f| {
                if Path::new(&f).is_absolute() {
                    PathBuf::from(f)
                } else {
                    root.join(f)
                }
            })
            .collect();
        let changed = arg_bool(args, "changed", false);
        let scan = match agent::scope(&root, &ws.project, &files, changed)? {
            Some(set) => ws.scan_files(&rules, &set),
            None => ws.scan_rules(&rules),
        };
        let lines_only = arg_bool(args, "changed_lines_only", false);
        let mut changed_lines: std::collections::HashMap<PathBuf, vcs::ChangedLines> = Default::default();
        let shown: Vec<&Finding> = scan
            .findings
            .iter()
            .filter(|f| !matches!(f.status, Status::Suppressed { .. }))
            .filter(|f| {
                !lines_only
                    || changed_lines
                        .entry(f.file.clone())
                        .or_insert_with(|| vcs::changed_lines(&root, &f.file))
                        .touches(f.span.start.line, f.span.end.line)
            })
            .collect();
        let actionable: Vec<&Finding> = shown.iter().copied().filter(|f| agent::actionable(f)).collect();
        let count = |pred: &dyn Fn(&Finding) -> bool| shown.iter().filter(|f| pred(f)).count();
        let summary = json!({
            "fixable": count(&|f| f.is_fixable()),
            "questions": count(&|f| matches!(f.status, Status::NeedsAnswer { .. })),
            "explained": count(&|f| matches!(f.status, Status::Explained)),
            "refused": count(&|f| matches!(f.status, Status::Refused { .. })),
            "declined": count(&|f| matches!(f.status, Status::Declined { .. })),
        });
        let text = if actionable.is_empty() {
            format!(
                "No issues to act on in {}{}.",
                if files.is_empty() && !changed {
                    "the project"
                } else {
                    "the files checked"
                },
                if shown.is_empty() {
                    String::new()
                } else {
                    format!(" ({} finding(s) already decided or refused by a guard)", shown.len())
                }
            )
        } else {
            let listed = &actionable[..actionable.len().min(MAX_LISTED)];
            let mut t = agent::agent_report(&ws.project, listed, "the code checked");
            if actionable.len() > MAX_LISTED {
                t.push_str(&format!(
                    "\n...and {} more; pass `files` to narrow the check.\n",
                    actionable.len() - MAX_LISTED
                ));
            }
            t
        };
        Ok(Output {
            text,
            structured: Some(json!({
                "project": root.display().to_string(),
                "summary": summary,
                "findings": shown.iter().take(500).map(|f| finding_value(&ws, f)).collect::<Vec<_>>(),
                "rule_errors": scan.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            })),
        })
    }

    fn fix(&self, args: &Value) -> Result<Output> {
        let root = self.project(args)?;
        let apply = arg_bool(args, "apply", false);
        let verify = arg_bool(args, "verify", true);
        if apply {
            match vcs::state(&root) {
                vcs::TreeState::Clean => {}
                vcs::TreeState::Dirty(paths) if !arg_bool(args, "allow_dirty", false) => bail!(
                    "the working tree has uncommitted changes ({}). Commit them first so the corrections can be reviewed on their own with `git diff`, or call again with allow_dirty=true.",
                    paths.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                ),
                vcs::TreeState::NotARepository | vcs::TreeState::Unknown => bail!(
                    "{} is not in a git repository, so the corrections could not be reviewed or undone; preview with apply=false instead",
                    root.display()
                ),
                _ => {}
            }
        }
        let mut ws = Workspace::open(&root, &[])?;
        let rules = arg_list(args, "rules");
        let (section, _) = ws.oracle_section();
        let oracle = (apply && verify).then(|| oracle_from(&root, &section));
        let (report, certificate) = crate::run_session(
            &root,
            &mut ws,
            &rules,
            crate::SessionMode {
                write: apply,
                max_steps: 500,
                proof: true,
                oracle: oracle.as_ref(),
                section: &section,
            },
            PLAIN,
            &mut |_| {},
        )?;
        let mut text = String::new();
        let verb = if apply {
            "Applied"
        } else {
            "Planned (preview, nothing written)"
        };
        let corrections: usize = report.steps.iter().map(|s| s.findings.len()).sum();
        text.push_str(&format!(
            "{verb}: {corrections} correction(s) in {} file(s).\n",
            report.files_changed.len()
        ));
        if apply && !report.verified && corrections > 0 {
            text.push_str("Not verified by a build or tests: none is configured or detected.\n");
        }
        for st in &report.steps {
            let oracle = st
                .oracle
                .as_ref()
                .map(|o| format!(" (tests {} -> {} passing)", o.passed_before, o.passed_after))
                .unwrap_or_default();
            text.push_str(&format!("  ✓ {} {}{oracle}\n", st.rule, st.files.join(", ")));
        }
        for r in &report.rejected {
            text.push_str(&format!(
                "  ✗ {} {}:{} rejected: {}\n",
                r.finding.rule,
                r.finding.file.display(),
                r.finding.span.start.line,
                r.reasons.join("; ")
            ));
        }
        let questions = report
            .remaining
            .iter()
            .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
            .count();
        if questions > 0 {
            text.push_str(&format!(
                "{questions} question(s) remain for the user; `check` lists them.\n"
            ));
        }
        if let Some(c) = &certificate {
            text.push_str(&format!("Proof certificate: {}\n", c.display()));
        }
        if !report.diff.is_empty() {
            text.push_str("\n```diff\n");
            text.push_str(&report.diff);
            text.push_str("```\n");
        }
        let mut structured: Value = serde_json::from_str(&crate::fix_json(&report)?)?;
        if let Some(c) = certificate {
            structured["certificate"] = json!(c.display().to_string());
        }
        Ok(Output {
            text,
            structured: Some(structured),
        })
    }

    fn answer(&self, args: &Value) -> Result<Output> {
        let root = self.project(args)?;
        let id = arg_str(args, "question_id").ok_or_else(|| anyhow!("question_id is required"))?;
        let answer = arg_str(args, "answer").ok_or_else(|| anyhow!("answer is required"))?;
        let note = arg_str(args, "note").map(String::from);
        let r = crate::record_answer(&root, id, answer, note)?;
        let then = if r.changes {
            "Call `fix` to apply the correction it asks for."
        } else {
            "The code stays as it is and will not be reported again."
        };
        Ok(Output {
            text: format!(
                "Recorded {} to {} in {}. {then}",
                r.answer,
                r.question,
                codemosis_engine::answers::FILE
            ),
            structured: Some(json!({"question": r.question, "answer": r.answer, "changes_code": r.changes})),
        })
    }

    fn explain(&self, args: &Value) -> Result<Output> {
        let rule = arg_str(args, "rule").ok_or_else(|| anyhow!("rule is required"))?;
        Ok(Output {
            text: crate::explain_text(rule, &[], PLAIN)?,
            structured: None,
        })
    }

    fn map(&self, args: &Value) -> Result<Output> {
        let root = self.project(args)?;
        let ws = Workspace::open(&root, &[])?;
        Ok(Output {
            text: crate::map::render(&ws, &root.display().to_string(), PLAIN),
            structured: None,
        })
    }

    fn rules(&self, args: &Value) -> Result<Output> {
        let query = arg_str(args, "query").map(str::to_lowercase);
        let packs = codemosis_core::pack::builtin().map_err(|e| anyhow!("{e}"))?;
        let mut list = Vec::new();
        for p in &packs {
            for r in &p.rules {
                let languages: Vec<&str> = if r.languages.is_empty() {
                    vec!["any"]
                } else {
                    r.languages.iter().map(|l| l.id()).collect()
                };
                let hay = format!("{} {} {}", r.id, r.spec.title, languages.join(" ")).to_lowercase();
                if query.as_ref().is_some_and(|q| !hay.contains(q.as_str())) {
                    continue;
                }
                let action = match (&r.spec.fix, &r.spec.ask) {
                    (Some(_), None) => "corrects",
                    (_, Some(_)) => "asks",
                    _ => "explains",
                };
                list.push(json!({"id": r.id, "title": r.spec.title, "category": r.spec.category.as_str(), "languages": languages, "action": action}));
            }
        }
        let text = list
            .iter()
            .map(|r| {
                format!(
                    "{}  {} [{}; {}; {}]",
                    r["id"].as_str().unwrap_or(""),
                    r["title"].as_str().unwrap_or(""),
                    r["category"].as_str().unwrap_or(""),
                    r["action"].as_str().unwrap_or(""),
                    r["languages"]
                        .as_array()
                        .map(|l| l.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "))
                        .unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Output {
            text: if text.is_empty() {
                "No rule matches.".into()
            } else {
                text
            },
            structured: Some(json!({"rules": list})),
        })
    }

    fn verify_proof(&self, args: &Value) -> Result<Output> {
        use codemosis_engine::proof::{verify, Certificate};
        let root = self.project(args)?;
        let cert_arg = arg_str(args, "certificate").ok_or_else(|| anyhow!("certificate is required"))?;
        let cert_path = if Path::new(cert_arg).is_absolute() {
            PathBuf::from(cert_arg)
        } else {
            root.join(cert_arg)
        };
        let cert = Certificate::load(&cert_path).map_err(|e| anyhow!(e))?;
        let (cfg, _) = codemosis_engine::config::Config::load(&root)?;
        let packs = cfg.load_packs(&root, &[])?;
        let mut project = codemosis_core::Project::load(&root, &cfg.filter())?;
        let formatters = codemosis_engine::formatter::Formatters::new(&root, &cfg.format, false);
        let v = verify(&cert, &root, &mut project, &packs, Some(&formatters));
        let mut text = String::new();
        for (name, r) in &v.checks {
            match r {
                Ok(()) => text.push_str(&format!("✓ {name}\n")),
                Err(why) => text.push_str(&format!("✗ {name}: {why}\n")),
            }
        }
        text.push_str(if v.ok() {
            "Verified: the change is exactly what the pinned rules prove."
        } else {
            "Not verified."
        });
        Ok(Output {
            text,
            structured: Some(json!({
                "verified": v.ok(),
                "checks": v.checks.iter().map(|(n, r)| json!({"check": n, "ok": r.is_ok(), "problem": r.as_ref().err()})).collect::<Vec<_>>(),
            })),
        })
    }

    fn call(&self, name: &str, args: &Value) -> Option<Result<Output>> {
        Some(match name {
            "check" => self.check(args),
            "fix" => self.fix(args),
            "answer" => self.answer(args),
            "explain" => self.explain(args),
            "map" => self.map(args),
            "rules" => self.rules(args),
            "verify_proof" => self.verify_proof(args),
            _ => return None,
        })
    }

    /// The reply to one request, or None for a notification.
    fn handle(&self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        // Notifications (initialized, cancelled, ...) need no reply.
        let id = id?;
        let result: std::result::Result<Value, (i64, String)> = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("");
                let version = if PROTOCOL_VERSIONS.contains(&asked) {
                    asked
                } else {
                    PROTOCOL_VERSIONS[0]
                };
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "codemosis", "title": "Reverse Codemosis", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => {
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                match self.call(name, &args) {
                    None => Err((-32602, format!("unknown tool: {name}"))),
                    Some(Ok(out)) => {
                        let mut r = json!({"content": [{"type": "text", "text": out.text}], "isError": false});
                        if let Some(s) = out.structured {
                            r["structuredContent"] = s;
                        }
                        Ok(r)
                    }
                    Some(Err(e)) => {
                        Ok(json!({"content": [{"type": "text", "text": format!("{e:#}")}], "isError": true}))
                    }
                }
            }
            "resources/list" => Ok(json!({"resources": []})),
            "prompts/list" => Ok(json!({"prompts": []})),
            _ => Err((-32601, format!("method not found: {method}"))),
        };
        Some(match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err((code, message)) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}),
        })
    }
}

/// Serve MCP on standard input and output until the client closes it.
pub fn serve(root: Option<PathBuf>) -> Result<()> {
    let root = match root {
        Some(r) => r,
        None => std::env::current_dir()?,
    };
    let root = root
        .canonicalize()
        .map_err(|e| anyhow!("cannot open {}: {e}", root.display()))?;
    codemosis_core::debug!("mcp: serving {}", root.display());
    let server = Server { root };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
            ),
            Ok(Value::Array(batch)) => {
                let replies: Vec<Value> = batch.iter().filter_map(|m| server.handle(m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(msg) => server.handle(&msg),
        };
        if let Some(r) = reply {
            let mut out = stdout.lock();
            writeln!(out, "{}", serde_json::to_string(&r)?)?;
            out.flush()?;
        }
    }
    Ok(())
}
