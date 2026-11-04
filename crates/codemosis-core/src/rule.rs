//! Compiled rules and matching.
//!
//! A rule is: a shape (tree-sitter query), facts that must hold (`where`),
//! and optionally a correction (`fix`) and a question (`ask`). Matching a rule
//! against a file yields findings; each finding carries the exact edits of its
//! correction, already checked by the engine's structural guards.

use crate::edit;
use crate::eval::{self, comments_in, eval, eval_bool, line_end, line_start, Bindings, EvalCtx, Value};
use crate::expr::{self, Expr};
use crate::lang::Language;
use crate::model::{Category, Edit, Evidence, Finding, Question, Span, Status};
use crate::pack::RuleSpec;
use crate::project::{Project, SourceFile};
use crate::tables::Tables;
use crate::template::{self, Template};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditOp {
    Replace,
    Delete,
    DeleteLines,
    DeleteItem,
    InsertBefore,
    InsertAfter,
    InsertLinesBefore,
    InsertLinesAfter,
    AddImport,
}

#[derive(Clone, Debug)]
struct CompiledEdit {
    op: EditOp,
    target: Option<String>,
    with: Option<Template>,
}

/// A failure inside a rule while it was being evaluated: a bug in the rule
/// or the engine, never in the code under examination.
#[derive(Clone, Debug)]
pub struct RuleError {
    pub rule: String,
    pub file: String,
    pub message: String,
}

impl fmt::Display for RuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rule {} on {}: {}", self.rule, self.file, self.message)
    }
}

pub struct CompiledRule {
    /// `pack/rule`.
    pub id: String,
    pub pack: String,
    pub spec: RuleSpec,
    pub language: &'static dyn Language,
    pub tables: Arc<Tables>,
    query: Query,
    lets: Vec<(String, Expr)>,
    facts: Vec<(String, Expr)>,
    edits: Vec<CompiledEdit>,
    summary: Template,
    question: Option<Template>,
    anchor: Vec<Template>,
    evidence: Vec<(String, Template)>,
}

impl fmt::Debug for CompiledRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompiledRule").field("id", &self.id).finish()
    }
}

fn capture_name(s: &str) -> Option<&str> {
    s.strip_prefix('@').filter(|n| !n.is_empty())
}

impl CompiledRule {
    pub fn compile(
        pack: &str,
        spec: RuleSpec,
        language: &'static dyn Language,
        tables: Arc<Tables>,
    ) -> Result<CompiledRule, Vec<String>> {
        let mut problems = Vec::new();
        if !crate::pack::valid_id(&spec.id) {
            problems.push(format!(
                "rule id '{}' must be lowercase letters, digits and dashes",
                spec.id
            ));
        }
        let query = match Query::new(&language.grammar(), &spec.shape) {
            Ok(q) => q,
            Err(e) => {
                problems.push(format!(
                    "shape does not compile for {} (line {}, column {}): {}",
                    language.id(),
                    e.row + 1,
                    e.column + 1,
                    e.message.trim()
                ));
                return Err(problems);
            }
        };
        let mut known: BTreeSet<String> = query.capture_names().iter().map(|s| s.to_string()).collect();

        let check_expr =
            |label: &str, src: &str, known: &BTreeSet<String>, problems: &mut Vec<String>| -> Option<Expr> {
                match expr::parse(src) {
                    Ok(e) => {
                        let mut caps = Vec::new();
                        e.captures(&mut caps);
                        for c in caps {
                            if !known.contains(&c) {
                                problems.push(format!("{label}: '{src}' uses unknown capture @{c}"));
                            }
                        }
                        let mut calls = Vec::new();
                        e.calls(&mut calls);
                        for c in calls {
                            if !eval::is_known_fact(&c, language) {
                                problems.push(format!(
                                    "{label}: '{src}' calls unknown fact '{c}' (see `codemosis facts`)"
                                ));
                            }
                        }
                        Some(e)
                    }
                    Err(err) => {
                        problems.push(format!("{label}: cannot parse '{src}': {err}"));
                        None
                    }
                }
            };

        let mut lets = Vec::new();
        for (name, value) in &spec.lets {
            let Some(src) = value.as_str() else {
                problems.push(format!("let.{name}: must be a string expression"));
                continue;
            };
            if let Some(e) = check_expr(&format!("let.{name}"), src, &known, &mut problems) {
                lets.push((name.clone(), e));
            }
            known.insert(name.clone());
        }
        let mut facts = Vec::new();
        for f in &spec.facts {
            if let Some(e) = check_expr("where", f, &known, &mut problems) {
                facts.push((f.clone(), e));
            }
        }

        let check_template = |label: &str, src: &str, problems: &mut Vec<String>| -> Option<Template> {
            match Template::parse(src) {
                Ok(t) => {
                    for n in t.names() {
                        if !known.contains(&n) {
                            problems.push(format!("{label}: template uses unknown capture ${{{n}}}"));
                        }
                    }
                    for f in t.filters() {
                        if !template::is_known_filter(&f, language) {
                            problems.push(format!("{label}: unknown filter '{f}'"));
                        }
                    }
                    Some(t)
                }
                Err(e) => {
                    problems.push(format!("{label}: {e}"));
                    None
                }
            }
        };

        let summary = check_template("summary", &spec.summary, &mut problems);
        let question = spec
            .ask
            .as_ref()
            .and_then(|a| check_template("ask.question", &a.question, &mut problems));
        let anchor: Vec<Template> = spec
            .anchor
            .iter()
            .filter_map(|a| check_template("anchor", a, &mut problems))
            .collect();
        let mut evidence = Vec::new();
        for ev in &spec.evidence {
            match capture_name(&ev.at) {
                Some(c) if known.contains(c) => {
                    if let Some(t) = check_template("evidence.label", &ev.label, &mut problems) {
                        evidence.push((c.to_string(), t));
                    }
                }
                _ => problems.push(format!("evidence.at: '{}' is not a known capture", ev.at)),
            }
        }
        if let Some(focus) = &spec.focus {
            if !capture_name(focus).is_some_and(|c| known.contains(c)) {
                problems.push(format!("focus: '{focus}' is not a known capture"));
            }
        }

        let mut edits = Vec::new();
        if let Some(fix) = &spec.fix {
            if fix.edits.is_empty() {
                problems.push("fix.edits is empty".to_string());
            }
            for (i, e) in fix.edits.iter().enumerate() {
                let label = format!("fix.edits[{i}]");
                let ops: Vec<(EditOp, &Option<String>)> = vec![
                    (EditOp::Replace, &e.replace),
                    (EditOp::Delete, &e.delete),
                    (EditOp::DeleteLines, &e.delete_lines),
                    (EditOp::DeleteItem, &e.delete_item),
                    (EditOp::InsertBefore, &e.insert_before),
                    (EditOp::InsertAfter, &e.insert_after),
                    (EditOp::InsertLinesBefore, &e.insert_lines_before),
                    (EditOp::InsertLinesAfter, &e.insert_lines_after),
                    (EditOp::AddImport, &e.add_import),
                ];
                let set: Vec<(EditOp, &String)> = ops
                    .into_iter()
                    .filter_map(|(op, v)| v.as_ref().map(|v| (op, v)))
                    .collect();
                if set.len() != 1 {
                    problems.push(format!("{label}: give exactly one operation (replace, delete, delete_lines, delete_item, insert_before, insert_after, insert_lines_before, insert_lines_after, add_import)"));
                    continue;
                }
                let (op, value) = set[0];
                let needs_with = matches!(
                    op,
                    EditOp::Replace
                        | EditOp::InsertBefore
                        | EditOp::InsertAfter
                        | EditOp::InsertLinesBefore
                        | EditOp::InsertLinesAfter
                );
                let compiled = if op == EditOp::AddImport {
                    if e.with.is_some() {
                        problems.push(format!("{label}: add_import takes the statement itself; remove 'with'"));
                    }
                    CompiledEdit {
                        op,
                        target: None,
                        with: check_template(&label, value, &mut problems),
                    }
                } else {
                    let target = match capture_name(value) {
                        Some(c) if known.contains(c) => Some(c.to_string()),
                        _ => {
                            problems.push(format!("{label}: '{value}' is not a known capture"));
                            None
                        }
                    };
                    let with = match (&e.with, needs_with) {
                        (Some(w), true) => check_template(&label, w, &mut problems),
                        (None, true) => {
                            problems.push(format!("{label}: this operation needs 'with'"));
                            None
                        }
                        (Some(_), false) => {
                            problems.push(format!("{label}: this operation takes no 'with'"));
                            None
                        }
                        (None, false) => None,
                    };
                    CompiledEdit { op, target, with }
                };
                edits.push(compiled);
            }
        }

        match (&spec.fix, spec.semantics, &spec.ask) {
            (Some(_), None, _) => problems.push(
                "a rule with a fix must declare semantics = \"preserved\" | \"corrected\" | \"repaired\"".to_string(),
            ),
            (None, Some(_), _) => problems.push("semantics only applies to rules with a fix".to_string()),
            (None, _, Some(_)) => problems.push("ask needs a fix to apply when the answer is yes".to_string()),
            _ => {}
        }
        if spec.examples.is_empty() {
            problems.push("every rule needs at least one [[example]]".to_string());
        }
        if spec.counterexamples.is_empty() {
            problems.push("every rule needs at least one [[counterexample]] showing where it stops".to_string());
        }
        for (i, ex) in spec.examples.iter().enumerate() {
            match (&spec.fix, &ex.after) {
                (Some(_), None) => problems.push(format!("example[{i}]: a rule with a fix needs 'after'")),
                (None, Some(_)) => problems.push(format!("example[{i}]: an explain-only rule takes no 'after'")),
                _ => {}
            }
        }

        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(CompiledRule {
            id: format!("{pack}/{}", spec.id),
            pack: pack.to_string(),
            spec,
            language,
            tables,
            query,
            lets,
            facts,
            edits,
            summary: summary.expect("checked"),
            question,
            anchor,
            evidence,
        })
    }

    pub fn category(&self) -> Category {
        self.spec.category
    }

    pub fn has_fix(&self) -> bool {
        self.spec.fix.is_some()
    }

    /// Find every place in `file` where this rule's shape appears and its
    /// facts hold.
    pub fn find(
        &self,
        file: &SourceFile,
        project: &Project,
        answers: &dyn Fn(&str) -> Option<bool>,
    ) -> Result<Vec<Finding>, RuleError> {
        if file.lang.id() != self.language.id() {
            return Ok(Vec::new());
        }
        let rule_err = |m: String| RuleError {
            rule: self.id.clone(),
            file: file.display_path(),
            message: m,
        };
        let names = self.query.capture_names();
        let mut raw: Vec<RawMatch> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, file.tree.root_node(), file.text.as_bytes());
        while let Some(m) = matches.next() {
            let mut captured: BTreeMap<String, Vec<Node>> = BTreeMap::new();
            for cap in m.captures {
                captured
                    .entry(names[cap.index as usize].to_string())
                    .or_default()
                    .push(cap.node);
            }
            let mut bindings: Bindings = BTreeMap::new();
            for (name, mut nodes) in captured {
                nodes.sort_by_key(|n| (n.start_byte(), n.end_byte()));
                nodes.dedup_by_key(|n| n.id());
                bindings.insert(name, Value::Nodes(nodes));
            }
            let mut ctx = EvalCtx {
                file,
                project,
                tables: &self.tables,
                bindings,
            };
            for (name, e) in &self.lets {
                let value = eval(e, &ctx).map_err(|er| rule_err(format!("let.{name}: {er}")))?;
                ctx.bindings.insert(name.clone(), value);
            }
            let mut holds = true;
            for (src, e) in &self.facts {
                if !eval_bool(e, &ctx).map_err(|er| rule_err(format!("where '{src}': {er}")))? {
                    holds = false;
                    break;
                }
            }
            if !holds {
                continue;
            }
            raw.push(self.raw_match(&ctx).map_err(rule_err)?);
        }

        // The same place can be matched by several alternatives of a shape.
        let mut seen = BTreeSet::new();
        raw.retain(|r| seen.insert((r.span.start_byte, r.span.end_byte, r.edits.clone())));
        raw.sort_by_key(|r| (r.span.start_byte, r.span.end_byte));

        // Anchor every match. Without explicit anchors, a match is identified
        // by where it lives and what it says, plus its occurrence number.
        let mut occurrences: BTreeMap<(String, String), usize> = BTreeMap::new();
        for r in raw.iter_mut() {
            if r.anchor.is_empty() {
                let key = (r.def_path.clone(), r.focus_text.clone());
                let n = occurrences.entry(key).or_insert(0);
                r.anchor = format!("{}|{}#{}", r.def_path, r.focus_text, n);
                *n += 1;
            }
        }

        // Merge matches that share an explicit anchor.
        let mut merged: Vec<RawMatch> = Vec::new();
        for r in raw {
            if !self.anchor.is_empty() {
                if let Some(existing) = merged.iter_mut().find(|m| m.anchor == r.anchor) {
                    existing.evidence.push(Evidence {
                        span: r.span.clone(),
                        label: "also here".to_string(),
                    });
                    existing.evidence.extend(r.evidence);
                    match (&mut existing.edits, r.edits) {
                        (Ok(a), Ok(b)) => {
                            a.extend(b);
                            a.sort();
                            a.dedup();
                        }
                        (Ok(_), Err(reason)) => existing.edits = Err(reason),
                        _ => {}
                    }
                    continue;
                }
            }
            merged.push(r);
        }

        let mut findings = Vec::new();
        for r in merged {
            let question = r.question.map(|text| Question {
                id: question_id(&self.id, &file.display_path(), &r.anchor),
                text,
            });
            let (status, edits) = match (self.has_fix(), r.edits) {
                (false, _) => (Status::Explained, Vec::new()),
                (true, Err(reason)) => (Status::Refused { reason }, Vec::new()),
                (true, Ok(edits)) => {
                    if let Err(e) = edit::normalize(&edits) {
                        return Err(rule_err(format!("the fix produces conflicting edits: {e}")));
                    }
                    match question {
                        None => (Status::Fixable, edits),
                        Some(q) => match answers(&q.id) {
                            Some(true) => (Status::Fixable, edits),
                            Some(false) => (Status::Declined { question: q }, Vec::new()),
                            None => (Status::NeedsAnswer { question: q }, edits),
                        },
                    }
                }
            };
            findings.push(Finding {
                rule: self.id.clone(),
                title: self.spec.title.clone(),
                category: self.spec.category,
                semantics: self.spec.semantics,
                file: file.path.clone(),
                span: r.span,
                message: r.message,
                evidence: r.evidence,
                anchor: r.anchor,
                status,
                edits,
            });
        }
        Ok(findings)
    }

    fn focus<'a>(&self, ctx: &EvalCtx<'a>) -> Option<Vec<Node<'a>>> {
        if let Some(name) = self.spec.focus.as_deref().and_then(capture_name) {
            return Some(ctx.bound_nodes(name)).filter(|n| !n.is_empty());
        }
        // Default: the capture covering the most text.
        ctx.bindings
            .values()
            .filter_map(|v| match v {
                Value::Nodes(n) if !n.is_empty() => Some(n),
                _ => None,
            })
            .max_by_key(|n| {
                let s = n.first().unwrap().start_byte();
                let e = n.last().unwrap().end_byte();
                (e - s, usize::MAX - s)
            })
            .cloned()
    }

    fn raw_match(&self, ctx: &EvalCtx<'_>) -> Result<RawMatch, String> {
        let focus = self.focus(ctx).ok_or("the match has no captured nodes to report")?;
        let span = Span::from_nodes(focus.first().unwrap(), focus.last().unwrap());
        let message = self.summary.render(ctx).map_err(|e| format!("summary: {e}"))?;
        let question = match &self.question {
            Some(t) => Some(t.render(ctx).map_err(|e| format!("ask.question: {e}"))?),
            None => None,
        };
        let mut anchor_parts = Vec::new();
        for t in &self.anchor {
            anchor_parts.push(t.render(ctx).map_err(|e| format!("anchor: {e}"))?);
        }
        let mut evidence = Vec::new();
        for (cap, label) in &self.evidence {
            if let Some(nodes) = Some(ctx.bound_nodes(cap)).filter(|n| !n.is_empty()) {
                evidence.push(Evidence {
                    span: Span::from_nodes(nodes.first().unwrap(), nodes.last().unwrap()),
                    label: label.render(ctx).map_err(|e| format!("evidence.label: {e}"))?,
                });
            }
        }
        let edits = if self.has_fix() {
            self.build_edits(ctx)?
        } else {
            Ok(Vec::new())
        };
        let focus_text: String = ctx.text_of(&focus).split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(RawMatch {
            span,
            message,
            question,
            evidence,
            edits,
            def_path: definition_path(ctx.file, focus.first().unwrap()),
            focus_text,
            anchor: anchor_parts.join("|"),
        })
    }

    /// Build the edits of the correction. The outer error is a rule bug; the
    /// inner error is a guard refusing a correction that is unsafe here.
    fn build_edits(&self, ctx: &EvalCtx<'_>) -> Result<Result<Vec<Edit>, String>, String> {
        let text = &ctx.file.text;
        let mut out = Vec::new();
        for (i, e) in self.edits.iter().enumerate() {
            let render = || -> Result<String, String> {
                e.with
                    .as_ref()
                    .map(|t| t.render(ctx).map_err(|er| format!("fix.edits[{i}]: {er}")))
                    .unwrap_or_else(|| Ok(String::new()))
            };
            if e.op == EditOp::AddImport {
                let stmt = render()?;
                match ctx.file.lang.add_import(ctx.file, &stmt) {
                    Ok(edit) => out.push(edit),
                    Err(reason) => return Ok(Err(reason)),
                }
                continue;
            }
            let target = e.target.as_deref().expect("compiled");
            let nodes = ctx.bound_nodes(target);
            let (Some(first), Some(last)) = (nodes.first(), nodes.last()) else {
                return Err(format!("fix.edits[{i}]: capture @{target} is empty in this match"));
            };
            let (start, end) = (first.start_byte(), last.end_byte());
            let edit = match e.op {
                EditOp::Replace => Edit {
                    start,
                    end,
                    text: render()?,
                },
                EditOp::Delete => Edit {
                    start,
                    end,
                    text: String::new(),
                },
                EditOp::InsertBefore => Edit {
                    start,
                    end: start,
                    text: render()?,
                },
                EditOp::InsertAfter => Edit {
                    start: end,
                    end,
                    text: render()?,
                },
                EditOp::DeleteLines => {
                    if !eval::occupies_lines(text, start, end) {
                        return Ok(Err("the code to delete shares its line with other code".to_string()));
                    }
                    let s = line_start(text, start);
                    let le = line_end(text, end);
                    let mut e2 = if le < text.len() { le + 1 } else { le };
                    // A block that stood alone between blank lines takes one
                    // of them with it, so no double gap is left behind. (A
                    // blank line beside a node that owns its lines cannot be
                    // inside a string, so this only ever touches whitespace.)
                    let prev_blank = s == 0 || text[line_start(text, s - 1)..s].trim().is_empty();
                    let next_end = line_end(text, e2);
                    if prev_blank && e2 < text.len() && text[e2..next_end].trim().is_empty() {
                        e2 = (next_end + 1).min(text.len());
                    }
                    Edit {
                        start: s,
                        end: e2,
                        text: String::new(),
                    }
                }
                EditOp::DeleteItem => {
                    let next = last.next_named_sibling().filter(|n| !n.is_extra());
                    let prev = first.prev_named_sibling().filter(|n| !n.is_extra());
                    let sep_after = next.is_some_and(|n| text[end..n.start_byte()].trim() == ",");
                    let sep_before = prev.is_some_and(|p| text[p.end_byte()..start].trim() == ",");
                    if sep_after {
                        Edit {
                            start,
                            end: next.unwrap().start_byte(),
                            text: String::new(),
                        }
                    } else if sep_before {
                        Edit {
                            start: prev.unwrap().end_byte(),
                            end,
                            text: String::new(),
                        }
                    } else {
                        return Ok(Err("the item is not part of a comma-separated list".to_string()));
                    }
                }
                EditOp::InsertLinesBefore | EditOp::InsertLinesAfter => {
                    let ls = line_start(text, start);
                    let le = line_end(text, end);
                    let before = &text[ls..start];
                    if !before.trim().is_empty() {
                        return Ok(Err(
                            "cannot insert lines: the anchor does not start its line".to_string()
                        ));
                    }
                    if e.op == EditOp::InsertLinesAfter && !text[end..le].trim().is_empty() {
                        return Ok(Err("cannot insert lines: the anchor does not end its line".to_string()));
                    }
                    let indent = before.to_string();
                    let mut block = String::new();
                    for line in render()?.lines() {
                        if !line.is_empty() {
                            block.push_str(&indent);
                        }
                        block.push_str(line);
                        block.push('\n');
                    }
                    if e.op == EditOp::InsertLinesBefore {
                        Edit {
                            start: ls,
                            end: ls,
                            text: block,
                        }
                    } else if le < text.len() {
                        Edit {
                            start: le + 1,
                            end: le + 1,
                            text: block,
                        }
                    } else {
                        Edit {
                            start: le,
                            end: le,
                            text: format!("\n{}", block.trim_end_matches('\n')),
                        }
                    }
                }
                EditOp::AddImport => unreachable!(),
            };
            out.push(edit);
        }
        // Guard: a correction never silently deletes a comment.
        for e in &out {
            if e.start == e.end {
                continue;
            }
            for c in comments_in(ctx.file, e.start, e.end) {
                let ctext = ctx.node_text(&c);
                if !e.text.contains(ctext) {
                    return Ok(Err(format!(
                        "the correction would delete the comment `{}` on line {}",
                        ctext.trim(),
                        c.start_position().row + 1
                    )));
                }
            }
        }
        Ok(Ok(out))
    }
}

struct RawMatch {
    span: Span,
    message: String,
    question: Option<String>,
    evidence: Vec<Evidence>,
    edits: Result<Vec<Edit>, String>,
    def_path: String,
    focus_text: String,
    anchor: String,
}

/// `Class.method` style path of the definitions enclosing `node`.
pub fn definition_path(file: &SourceFile, node: &Node) -> String {
    let mut parts = Vec::new();
    let mut cur = node.parent();
    while let Some(n) = cur {
        if let Some(name) = file.lang.definition_name(&n, &file.text) {
            parts.push(name);
        }
        cur = n.parent();
    }
    parts.reverse();
    parts.join(".")
}

/// FNV-1a: small, stable across platforms and versions.
pub fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn question_id(rule: &str, file: &str, anchor: &str) -> String {
    let h = fnv1a(format!("{rule}\u{0}{file}\u{0}{anchor}").as_bytes());
    format!("Q-{:08x}", h >> 32)
}

/// Every finding of every rule in every file.
#[derive(Debug, Default)]
pub struct Scan {
    pub findings: Vec<Finding>,
    pub errors: Vec<RuleError>,
}

pub fn scan<'r>(
    project: &Project,
    rules: impl IntoIterator<Item = &'r CompiledRule> + Clone,
    answers: &dyn Fn(&str) -> Option<bool>,
    only_files: Option<&BTreeSet<usize>>,
) -> Scan {
    let mut out = Scan::default();
    for (idx, file) in project.files.iter().enumerate() {
        if only_files.is_some_and(|set| !set.contains(&idx)) {
            continue;
        }
        for rule in rules.clone() {
            match rule.find(file, project, answers) {
                Ok(f) => out.findings.extend(f),
                Err(e) => out.errors.push(e),
            }
        }
    }
    supersede(&mut out.findings);
    sort_findings(&mut out.findings);
    out
}

/// A finding that carries a correction supersedes an explain-only finding
/// at exactly the same place: the explanation adds nothing the correction
/// does not already say.
fn supersede(findings: &mut Vec<Finding>) {
    let corrected: BTreeSet<(std::path::PathBuf, usize, usize)> = findings
        .iter()
        .filter(|f| !matches!(f.status, Status::Explained))
        .map(|f| (f.file.clone(), f.span.start_byte, f.span.end_byte))
        .collect();
    findings.retain(|f| {
        !matches!(f.status, Status::Explained)
            || !corrected.contains(&(f.file.clone(), f.span.start_byte, f.span.end_byte))
    });
}

pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        (a.category, &a.file, a.span.start_byte, &a.rule).cmp(&(b.category, &b.file, b.span.start_byte, &b.rule))
    });
}
