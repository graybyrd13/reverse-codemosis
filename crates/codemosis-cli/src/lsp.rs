//! `codemosis lsp`: Reverse Codemosis in any editor that speaks the
//! Language Server Protocol.
//!
//! * Findings are published as diagnostics as you type, with the project's
//!   own configuration, suppressions and severities.
//! * Hovering a finding shows the rule's explanation and question.
//! * A proven correction is a quick fix; "fix all" applies every proven
//!   correction in the file that does not overlap another.
//! * A question's answers are code actions: choosing one records it in
//!   `.codemosis/answers.toml` (and the audit log), exactly as
//!   `codemosis answer` would, and applies the correction it unlocks.
//!
//! The server never runs the project's build or tests and never writes
//! source files itself: the editor applies the edits, and `codemosis fix`
//! remains the command that verifies corrections against the oracle.

use anyhow::{anyhow, Context, Result};
use codemosis_core::{edit, Finding, SourceFile, Status};
use codemosis_engine::answers::Answer;
use codemosis_engine::policy::Level;
use codemosis_engine::{enabled_rules, Workspace};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability, Command,
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams, ExecuteCommandOptions,
    ExecuteCommandParams, Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams, Location,
    MarkupContent, MarkupKind, NumberOrString, Position, PublishDiagnosticsParams, Range, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url, WorkspaceEdit,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

const ANSWER: &str = "codemosis.answer";

/// A byte offset as an LSP position (0-based line, UTF-16 column).
pub fn position(text: &str, offset: usize) -> Position {
    let (line, col) = codemosis_engine::ci::line_col_utf16(text, offset);
    Position::new((line - 1) as u32, (col - 1) as u32)
}

/// An LSP position as a byte offset, clamped to the text.
pub fn offset(text: &str, pos: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..pos.line {
        match text[line_start..].find('\n') {
            Some(i) => line_start += i + 1,
            None => return text.len(),
        }
    }
    let line_end = text[line_start..]
        .find('\n')
        .map(|i| line_start + i)
        .unwrap_or(text.len());
    let mut units = 0;
    for (i, ch) in text[line_start..line_end].char_indices() {
        if units >= pos.character as usize {
            return line_start + i;
        }
        units += ch.len_utf16();
    }
    line_end
}

fn range(text: &str, start: usize, end: usize) -> Range {
    Range::new(position(text, start), position(text, end))
}

struct Server {
    root: PathBuf,
    ws: Workspace,
    /// Open documents: relative path to URI.
    open: BTreeMap<PathBuf, Url>,
    findings: HashMap<PathBuf, Vec<Finding>>,
    next_id: i32,
}

impl Server {
    fn rel(&self, uri: &Url) -> Option<PathBuf> {
        let path = uri.to_file_path().ok()?;
        let rel = path.strip_prefix(&self.root).ok()?;
        Some(PathBuf::from(rel.to_string_lossy().replace('\\', "/")))
    }

    fn set_text(&mut self, rel: &Path, text: String) {
        match self.ws.project.index_of(rel) {
            Some(i) => self.ws.project.set_text(i, text),
            None => self.ws.project.files.push(SourceFile::new(rel, text)),
        }
    }

    fn scan(&mut self, rel: &Path) -> Vec<Finding> {
        let Some(idx) = self.ws.project.index_of(rel) else {
            return Vec::new();
        };
        let rules = enabled_rules(&self.ws.packs, &self.ws.config, &self.ws.project, &[]);
        let answers = |id: &str| self.ws.answers.get(id);
        let only = BTreeSet::from([idx]);
        let mut scan = codemosis_core::scan(&self.ws.project, rules, &answers, Some(&only));
        self.ws
            .policy
            .apply_suppressions(&mut scan.findings, &codemosis_core::suppress::today());
        scan.findings.retain(|f| f.is_open());
        scan.findings
    }

    fn text(&self, rel: &Path) -> &str {
        self.ws
            .project
            .index_of(rel)
            .map(|i| self.ws.project.files[i].text.as_str())
            .unwrap_or("")
    }

    fn diagnostic(&self, rel: &Path, f: &Finding, uri: &Url) -> Diagnostic {
        let text = self.text(rel);
        let severity = match (self.ws.policy.level(&f.rule, f.category), f.category) {
            (Level::Error, _) => DiagnosticSeverity::ERROR,
            (Level::Warning, _) => DiagnosticSeverity::WARNING,
            (Level::Note, codemosis_core::Category::Simplification) => DiagnosticSeverity::HINT,
            (Level::Note, _) => DiagnosticSeverity::INFORMATION,
        };
        let note = match &f.status {
            Status::Fixable => " (a proven correction is available)".to_string(),
            Status::NeedsAnswer { question } => format!(" — {} ({})", question.text, question.choices()),
            Status::Refused { reason } => format!(" (correction refused: {reason})"),
            _ => String::new(),
        };
        Diagnostic {
            range: range(text, f.span.start_byte, f.span.end_byte),
            severity: Some(severity),
            code: Some(NumberOrString::String(f.rule.clone())),
            source: Some("codemosis".into()),
            message: format!("{}{note}", f.message),
            related_information: (!f.evidence.is_empty()).then(|| {
                f.evidence
                    .iter()
                    .map(|e| DiagnosticRelatedInformation {
                        location: Location::new(uri.clone(), range(text, e.span.start_byte, e.span.end_byte)),
                        message: e.label.clone(),
                    })
                    .collect()
            }),
            data: Some(serde_json::json!({ "fingerprint": f.fingerprint() })),
            ..Default::default()
        }
    }

    fn publish(&mut self, conn: &Connection, rel: &Path) -> Result<()> {
        let Some(uri) = self.open.get(rel).cloned() else {
            return Ok(());
        };
        let findings = self.scan(rel);
        let diagnostics = findings.iter().map(|f| self.diagnostic(rel, f, &uri)).collect();
        self.findings.insert(rel.to_path_buf(), findings);
        let params = PublishDiagnosticsParams::new(uri, diagnostics, None);
        conn.sender.send(Message::Notification(Notification::new(
            "textDocument/publishDiagnostics".into(),
            params,
        )))?;
        Ok(())
    }

    fn text_edits(&self, rel: &Path, edits: &[codemosis_core::Edit]) -> Vec<TextEdit> {
        let text = self.text(rel);
        edits
            .iter()
            .map(|e| TextEdit::new(range(text, e.start, e.end), e.text.clone()))
            .collect()
    }

    fn workspace_edit(&self, uri: &Url, edits: Vec<TextEdit>) -> WorkspaceEdit {
        WorkspaceEdit {
            changes: Some(HashMap::from([(uri.clone(), edits)])),
            ..Default::default()
        }
    }

    fn code_actions(&self, params: CodeActionParams) -> Vec<CodeActionOrCommand> {
        let uri = params.text_document.uri;
        let Some(rel) = self.rel(&uri) else { return Vec::new() };
        let Some(findings) = self.findings.get(&rel) else {
            return Vec::new();
        };
        let text = self.text(&rel);
        let (a, b) = (offset(text, params.range.start), offset(text, params.range.end));
        let mut out = Vec::new();
        for f in findings
            .iter()
            .filter(|f| f.span.start_byte <= b && a <= f.span.end_byte)
        {
            let diag = self.diagnostic(&rel, f, &uri);
            match &f.status {
                Status::Fixable => {
                    let sem = f.semantics.map(|s| format!(", {}", s.as_str())).unwrap_or_default();
                    out.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Correct: {} (proven{sem})", f.title),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag]),
                        edit: Some(self.workspace_edit(&uri, self.text_edits(&rel, &f.edits))),
                        is_preferred: Some(true),
                        ..Default::default()
                    }));
                }
                Status::NeedsAnswer { question } => {
                    for o in question.options.iter().filter(|o| o.unavailable.is_none()) {
                        out.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("{} {}: {}", question.text, o.answer, o.means),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            command: Some(Command::new(
                                format!("Answer {}", o.answer),
                                ANSWER.into(),
                                Some(vec![
                                    serde_json::json!(question.id),
                                    serde_json::json!(o.answer),
                                    serde_json::json!(uri.as_str()),
                                ]),
                            )),
                            ..Default::default()
                        }));
                    }
                }
                _ => {}
            }
        }
        let wants_fix_all = params
            .context
            .only
            .as_ref()
            .is_none_or(|kinds| kinds.iter().any(|k| k.as_str().starts_with("source")));
        if wants_fix_all {
            let mut chosen: Vec<codemosis_core::Edit> = Vec::new();
            let mut n = 0;
            for f in findings.iter().filter(|f| f.is_fixable()) {
                if !f.edits.iter().any(|e| chosen.iter().any(|c| edit::overlaps(e, c))) {
                    chosen.extend(f.edits.iter().cloned());
                    n += 1;
                }
            }
            if n > 0 {
                out.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Apply {n} proven correction(s) in this file"),
                    kind: Some(CodeActionKind::new("source.fixAll.codemosis")),
                    edit: Some(self.workspace_edit(&uri, self.text_edits(&rel, &chosen))),
                    ..Default::default()
                }));
            }
        }
        out
    }

    fn hover(&self, params: HoverParams) -> Option<Hover> {
        let uri = params.text_document_position_params.text_document.uri;
        let rel = self.rel(&uri)?;
        let text = self.text(&rel);
        let at = offset(text, params.text_document_position_params.position);
        let f = self
            .findings
            .get(&rel)?
            .iter()
            .find(|f| f.span.start_byte <= at && at <= f.span.end_byte)?;
        let rule = self
            .ws
            .packs
            .iter()
            .flat_map(|p| p.rules.iter())
            .find(|r| r.id == f.rule);
        let mut md = format!("**{}** · `{}` · {}\n\n{}", f.title, f.rule, f.category, f.message);
        if let Some(r) = rule {
            md.push_str(&format!("\n\n---\n\n{}", r.spec.explain.trim()));
            if !r.spec.cwe.is_empty() {
                md.push_str(&format!("\n\n{}", r.spec.cwe.join(", ")));
            }
        }
        if let Some(q) = f.question() {
            md.push_str(&format!("\n\n**{}**", q.text));
            for o in &q.options {
                md.push_str(&format!("\n- **{}**: {}", o.answer, o.means));
            }
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: Some(range(text, f.span.start_byte, f.span.end_byte)),
        })
    }

    /// Record an answer, rescan, and ask the editor to apply what it unlocks.
    fn answer(&mut self, conn: &Connection, args: &[serde_json::Value]) -> Result<()> {
        let id = args
            .first()
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing question id"))?;
        let answer = args
            .get(1)
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("missing answer"))?;
        let uri = args
            .get(2)
            .and_then(|v| v.as_str())
            .and_then(|u| Url::parse(u).ok())
            .ok_or_else(|| anyhow!("missing document"))?;
        let rel = self
            .rel(&uri)
            .ok_or_else(|| anyhow!("the document is outside the project"))?;
        let f = self
            .findings
            .get(&rel)
            .and_then(|fs| fs.iter().find(|f| f.question().is_some_and(|q| q.id == id)))
            .cloned()
            .ok_or_else(|| anyhow!("no open question {id}"))?;
        let q = f.question().expect("found by question").clone();
        let option = q
            .option(answer)
            .ok_or_else(|| anyhow!("{answer} is not an answer to {id}"))?;
        self.ws.answers.set(Answer {
            id: q.id.clone(),
            rule: f.rule.clone(),
            location: format!("{}:{}", f.file.display(), f.span.start.line),
            question: q.text.clone(),
            answer: option.answer.clone(),
            note: Some("answered in the editor".into()),
        });
        self.ws.answers.save(&self.root).map_err(|e| anyhow!(e))?;
        let _ = codemosis_engine::audit::record(
            &self.root,
            "answer",
            serde_json::json!({"id": q.id, "rule": f.rule, "answer": option.answer, "via": "lsp"}),
        );
        self.publish(conn, &rel)?;
        if option.changes {
            if let Some(now) = self.findings.get(&rel).and_then(|fs| {
                fs.iter()
                    .find(|g| g.fingerprint() == f.fingerprint() && g.is_fixable())
                    .cloned()
            }) {
                let edit = self.workspace_edit(&uri, self.text_edits(&rel, &now.edits));
                self.next_id += 1;
                let req = Request::new(
                    RequestId::from(self.next_id),
                    "workspace/applyEdit".into(),
                    lsp_types::ApplyWorkspaceEditParams {
                        label: Some(format!("{} ({})", f.title, option.answer)),
                        edit,
                    },
                );
                conn.sender.send(Message::Request(req))?;
            }
        }
        Ok(())
    }
}

fn reply<T: serde::Serialize>(conn: &Connection, id: RequestId, result: T) -> Result<()> {
    conn.sender
        .send(Message::Response(Response::new_ok(id, result)))
        .context("send a response")
}

pub fn serve() -> Result<()> {
    let (conn, io) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![ANSWER.into()],
            ..Default::default()
        }),
        ..Default::default()
    };
    let init: InitializeParams = serde_json::from_value(conn.initialize(serde_json::to_value(caps)?)?)?;
    #[allow(deprecated)]
    let root = init
        .workspace_folders
        .as_ref()
        .and_then(|w| w.first())
        .and_then(|w| w.uri.to_file_path().ok())
        .or_else(|| init.root_uri.as_ref().and_then(|u| u.to_file_path().ok()))
        .map(Ok)
        .unwrap_or_else(std::env::current_dir)?;
    let root = root.canonicalize().unwrap_or(root);
    let mut ws = Workspace::open(&root, &[])?;
    // Every pack, so a file in a language the project did not have yet works.
    ws.packs = ws.config.load_packs(&root, &[])?;
    let mut server = Server {
        root,
        ws,
        open: BTreeMap::new(),
        findings: HashMap::new(),
        next_id: 0,
    };

    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    break;
                }
                match req.method.as_str() {
                    "textDocument/codeAction" => {
                        let p: CodeActionParams = serde_json::from_value(req.params)?;
                        reply(&conn, req.id, server.code_actions(p))?;
                    }
                    "textDocument/hover" => {
                        let p: HoverParams = serde_json::from_value(req.params)?;
                        reply(&conn, req.id, server.hover(p))?;
                    }
                    "workspace/executeCommand" => {
                        let p: ExecuteCommandParams = serde_json::from_value(req.params)?;
                        let result = if p.command == ANSWER {
                            server.answer(&conn, &p.arguments)
                        } else {
                            Err(anyhow!("unknown command {}", p.command))
                        };
                        match result {
                            Ok(()) => reply(&conn, req.id, serde_json::Value::Null)?,
                            Err(e) => conn.sender.send(Message::Response(Response::new_err(
                                req.id,
                                lsp_server::ErrorCode::InvalidParams as i32,
                                e.to_string(),
                            )))?,
                        }
                    }
                    _ => conn.sender.send(Message::Response(Response::new_err(
                        req.id,
                        lsp_server::ErrorCode::MethodNotFound as i32,
                        format!("{} is not supported", req.method),
                    )))?,
                }
            }
            Message::Notification(n) => match n.method.as_str() {
                "textDocument/didOpen" => {
                    let p: DidOpenTextDocumentParams = serde_json::from_value(n.params)?;
                    if let Some(rel) = server.rel(&p.text_document.uri) {
                        server.open.insert(rel.clone(), p.text_document.uri);
                        server.set_text(&rel, p.text_document.text);
                        server.publish(&conn, &rel)?;
                    }
                }
                "textDocument/didChange" => {
                    let p: DidChangeTextDocumentParams = serde_json::from_value(n.params)?;
                    if let (Some(rel), Some(change)) = (server.rel(&p.text_document.uri), p.content_changes.last()) {
                        server.set_text(&rel, change.text.clone());
                        server.publish(&conn, &rel)?;
                    }
                }
                "textDocument/didSave" => {
                    let p: DidSaveTextDocumentParams = serde_json::from_value(n.params)?;
                    if let Some(rel) = server.rel(&p.text_document.uri) {
                        if let Some(text) = p.text {
                            server.set_text(&rel, text);
                        }
                        server.publish(&conn, &rel)?;
                    }
                }
                "textDocument/didClose" => {
                    let p: DidCloseTextDocumentParams = serde_json::from_value(n.params)?;
                    if let Some(rel) = server.rel(&p.text_document.uri) {
                        server.open.remove(&rel);
                        server.findings.remove(&rel);
                        conn.sender.send(Message::Notification(Notification::new(
                            "textDocument/publishDiagnostics".into(),
                            PublishDiagnosticsParams::new(p.text_document.uri, Vec::new(), None),
                        )))?;
                    }
                }
                _ => {}
            },
            Message::Response(_) => {}
        }
    }
    // The writer thread ends only when the connection is gone: drop it
    // before joining, or shutdown would wait forever.
    drop(conn);
    io.join()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip_through_utf16() {
        let t = "a = 'é'\n😀 = 1\nlast";
        for i in (0..=t.len()).filter(|i| t.is_char_boundary(*i)) {
            assert_eq!(offset(t, position(t, i)), i, "offset {i}");
        }
        assert_eq!(position(t, t.find('=').unwrap()), Position::new(0, 2));
        assert_eq!(position(t, t.rfind('=').unwrap()), Position::new(1, 3));
    }
}
