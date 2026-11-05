//! Compiled rules and matching.
//!
//! A rule is a shape, facts that must hold (`where`), and optionally a
//! correction (`fix`) or a question (`ask`) whose answers may carry their own
//! corrections. A shape is either a tree-sitter query, for languages with a
//! grammar, or a regular expression over the text, for any language; text
//! matches are only accepted where they start in code (unless the rule says
//! otherwise), and only in files whose comments and strings can be told
//! apart. Matching yields findings; each finding carries the exact edits of
//! every correction it offers, already checked by the engine's guards.

use crate::edit;
use crate::eval::{self, comments_in, eval, eval_bool, line_end, line_start, Bindings, EvalCtx, Value};
use crate::expr::{self, Expr};
use crate::lang::Language;
use crate::lex::SegKind;
use crate::model::{Category, Edit, Evidence, Finding, Question, QuestionOption, Span, Status};
use crate::pack::{EditSpec, RuleSpec};
use crate::project::{Project, SourceFile};
use crate::redact::redact;
use crate::tables::{Table, Tables};
use crate::template::{self, Template};
use regex::Regex;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Within {
    Code,
    Comment,
    Str,
    Any,
}

enum Shape {
    /// One query per grammar the rule is written for.
    Tree(Vec<(&'static str, Query)>),
    Pattern(Regex),
    /// Regular expressions per GitHub language name, from a pack table.
    Table(BTreeMap<String, Vec<Regex>>),
}

struct CompiledOption {
    answer: String,
    means: String,
    edits: Vec<CompiledEdit>,
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
    /// The grammars the rule is written for; empty for text rules of a text
    /// pack, which read any file.
    pub languages: Vec<&'static dyn Language>,
    pub tables: Arc<Tables>,
    shape: Shape,
    within: Within,
    lets: Vec<(String, Expr)>,
    facts: Vec<(String, Expr)>,
    fix: Option<Vec<CompiledEdit>>,
    options: Vec<CompiledOption>,
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

/// Tools whose suppression comments the engine understands (see `foreign`).
pub const KNOWN_TOOLS: &[&str] = &[
    "flake8",
    "ruff",
    "mypy",
    "pyright",
    "pylint",
    "eslint",
    "clang-tidy",
    "golangci",
    "shellcheck",
    "clippy",
];

/// The most partial matches a shape may have in progress at once in a file.
pub const MATCH_LIMIT: u32 = 1024;

const EDIT_OPS: &str =
    "replace, delete, delete_lines, delete_item, insert_before, insert_after, insert_lines_before, insert_lines_after, add_import";

impl CompiledRule {
    pub fn compile(
        pack: &str,
        spec: RuleSpec,
        languages: &[&'static dyn Language],
        tables: Arc<Tables>,
    ) -> Result<CompiledRule, Vec<String>> {
        let mut problems = Vec::new();
        if !crate::pack::valid_id(&spec.id) {
            problems.push(format!(
                "rule id '{}' must be lowercase letters, digits and dashes",
                spec.id
            ));
        }
        let mut known: BTreeSet<String> = BTreeSet::new();

        let shape = match (&spec.shape, &spec.pattern, &spec.patterns) {
            (Some(q), None, None) => {
                if languages.is_empty() {
                    problems.push(
                        "a `shape` needs a grammar: text packs (language = \"*\") use `pattern` or `patterns`".into(),
                    );
                    return Err(problems);
                }
                let mut queries = Vec::new();
                for lang in languages {
                    match Query::new(&lang.grammar(), q) {
                        Ok(query) => {
                            known.extend(query.capture_names().iter().map(|s| s.to_string()));
                            queries.push((lang.id(), query));
                        }
                        Err(e) => problems.push(format!(
                            "shape does not compile for {} (line {}, column {}): {}",
                            lang.id(),
                            e.row + 1,
                            e.column + 1,
                            e.message.trim()
                        )),
                    }
                }
                if !problems.is_empty() {
                    return Err(problems);
                }
                Shape::Tree(queries)
            }
            (None, Some(p), None) => match Regex::new(p) {
                Ok(re) => {
                    known.insert("match".into());
                    known.extend(re.capture_names().flatten().map(|s| s.to_string()));
                    Shape::Pattern(re)
                }
                Err(e) => {
                    problems.push(format!("pattern is not a valid regular expression: {e}"));
                    return Err(problems);
                }
            },
            (None, None, Some(table_name)) => {
                let mut map = BTreeMap::new();
                match tables.get(table_name) {
                    Some(Table::Map(entries)) => {
                        known.insert("match".into());
                        for (lang_name, patterns) in entries {
                            if crate::linguist::by_name(lang_name).is_none() {
                                problems.push(format!(
                                    "patterns table '{table_name}': '{lang_name}' is not a language GitHub knows"
                                ));
                            }
                            let mut compiled = Vec::new();
                            for p in patterns {
                                match Regex::new(p) {
                                    Ok(re) => {
                                        known.extend(re.capture_names().flatten().map(|s| s.to_string()));
                                        compiled.push(re);
                                    }
                                    Err(e) => problems.push(format!("patterns table '{table_name}', {lang_name}: {e}")),
                                }
                            }
                            map.insert(lang_name.clone(), compiled);
                        }
                    }
                    Some(Table::Set(_)) => {
                        problems.push(format!("patterns: table '{table_name}' must map languages to patterns"))
                    }
                    None => problems.push(format!("patterns: the pack has no table named '{table_name}'")),
                }
                Shape::Table(map)
            }
            _ => {
                problems.push("give exactly one of `shape` (a tree-sitter query), `pattern` or `patterns`".into());
                return Err(problems);
            }
        };
        let within = match spec.within.as_deref() {
            None | Some("code") => Within::Code,
            Some("comment") => Within::Comment,
            Some("string") => Within::Str,
            Some("any") => Within::Any,
            Some(other) => {
                problems.push(format!("within: '{other}' must be code, comment, string or any"));
                Within::Code
            }
        };
        if spec.within.is_some() && matches!(shape, Shape::Tree(_)) {
            problems.push("within only applies to text rules (pattern or patterns)".into());
        }
        if let (Shape::Table(map), true) = (&shape, within != Within::Any) {
            for name in map.keys() {
                let readable = crate::linguist::by_name(name).is_some_and(|l| {
                    crate::lex::profile_for(l).is_some() || crate::lang::for_linguist(l.name).is_some()
                });
                if !readable {
                    problems.push(format!(
                        "patterns: {name} has no known comment syntax, so a match that must start in code can never be found there (add a lexical profile, or use within = \"any\")"
                    ));
                }
            }
        }

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
                            if !eval::is_known_fact(&c, languages) {
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
                        if !template::is_known_filter(&f, languages) {
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

        let compile_edits = |label: &str, specs: &[EditSpec], problems: &mut Vec<String>| -> Vec<CompiledEdit> {
            let mut edits = Vec::new();
            for (i, e) in specs.iter().enumerate() {
                let label = format!("{label}[{i}]");
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
                    problems.push(format!("{label}: give exactly one operation ({EDIT_OPS})"));
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
                        with: check_template(&label, value, problems),
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
                        (Some(w), true) => check_template(&label, w, problems),
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
            edits
        };

        let fix = spec.fix.as_ref().map(|f| {
            if f.edits.is_empty() {
                problems.push("fix.edits is empty".into());
            }
            compile_edits("fix.edits", &f.edits, &mut problems)
        });
        let mut options = Vec::new();
        if let Some(ask) = &spec.ask {
            for (i, o) in ask.options.iter().enumerate() {
                if o.answer.is_empty() || !o.answer.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                    problems.push(format!(
                        "ask.option[{i}]: answer '{}' must be a lowercase word",
                        o.answer
                    ));
                }
                if ask.options.iter().filter(|p| p.answer == o.answer).count() > 1 {
                    problems.push(format!("ask.option[{i}]: answer '{}' is given twice", o.answer));
                }
                options.push(CompiledOption {
                    answer: o.answer.clone(),
                    means: o.means.clone(),
                    edits: compile_edits(&format!("ask.option[{i}].edits"), &o.edits, &mut problems),
                });
            }
            if !ask.options.is_empty() {
                if ask.options.len() < 2 {
                    problems.push("ask: a question with options needs at least two".into());
                }
                if !ask.options.iter().any(|o| !o.edits.is_empty()) {
                    problems.push("ask: at least one option must change the code".into());
                }
                if fix.is_some() {
                    problems.push("ask: options carry their own edits; remove [fix]".into());
                }
            }
        }
        let changes = fix.is_some() || options.iter().any(|o| !o.edits.is_empty());

        match (changes, spec.semantics, &spec.ask) {
            (true, None, _) => problems.push(
                "a rule that changes code must declare semantics = \"preserved\" | \"corrected\" | \"repaired\"".into(),
            ),
            (false, Some(_), _) => problems.push("semantics only applies to rules that change code".into()),
            (false, _, Some(_)) => problems.push("ask needs a [fix] to apply on yes, or options with edits".into()),
            _ => {}
        }
        if spec.examples.is_empty() {
            problems.push("every rule needs at least one [[example]]".into());
        }
        if spec.counterexamples.is_empty() {
            problems.push("every rule needs at least one [[counterexample]] showing where it stops".into());
        }
        for (i, ex) in spec.examples.iter().enumerate() {
            match (changes, &ex.after) {
                (true, None) => problems.push(format!("example[{i}]: a rule that changes code needs 'after'")),
                (false, Some(_)) => problems.push(format!("example[{i}]: an explain-only rule takes no 'after'")),
                _ => {}
            }
            if let Some(a) = &ex.answer {
                let valid = if options.is_empty() {
                    a == "yes" || a == "no"
                } else {
                    options.iter().any(|o| o.answer == *a)
                };
                if spec.ask.is_none() || !valid {
                    problems.push(format!("example[{i}]: '{a}' is not an answer to this rule's question"));
                }
            }
        }
        let cwe_re = Regex::new(r"^CWE-\d+$").expect("valid regex");
        for c in &spec.cwe {
            if !cwe_re.is_match(c) {
                problems.push(format!("cwe: '{c}' is not like CWE-396"));
            }
        }
        for e in &spec.equivalents {
            match e.split_once(':') {
                Some((tool, code)) if KNOWN_TOOLS.contains(&tool) && !code.is_empty() => {}
                _ => problems.push(format!(
                    "equivalents: '{e}' is not `tool:code` with a tool among {}",
                    KNOWN_TOOLS.join(", ")
                )),
            }
        }
        if spec.replaced_by.is_some() && spec.deprecated.is_none() {
            problems.push("replaced_by only applies to a deprecated rule; add deprecated = \"why\"".into());
        }
        for (label, path) in spec
            .examples
            .iter()
            .map(|e| ("example", &e.path))
            .chain(spec.counterexamples.iter().map(|c| ("counterexample", &c.path)))
        {
            if let Some(p) = path {
                if !is_plain_relative(p) {
                    problems.push(format!(
                        "{label}: path '{p}' must be a relative path inside the example project (no '..', no leading '/')"
                    ));
                }
            }
        }
        for (label, l) in spec
            .examples
            .iter()
            .map(|e| ("example", &e.language))
            .chain(spec.counterexamples.iter().map(|c| ("counterexample", &c.language)))
        {
            if let Some(l) = l {
                if !languages.iter().any(|g| g.id() == l) {
                    problems.push(format!("{label}: language '{l}' is not one of this pack's grammars"));
                }
            }
        }

        if !problems.is_empty() {
            return Err(problems);
        }
        Ok(CompiledRule {
            id: format!("{pack}/{}", spec.id),
            pack: pack.to_string(),
            spec,
            languages: languages.to_vec(),
            tables,
            shape,
            within,
            lets,
            facts,
            fix,
            options,
            summary: summary.expect("checked"),
            question,
            anchor,
            evidence,
        })
    }

    pub fn category(&self) -> Category {
        self.spec.category
    }

    /// True when the rule can change code (directly or through an answer).
    pub fn changes_code(&self) -> bool {
        self.fix.is_some() || self.options.iter().any(|o| !o.edits.is_empty())
    }

    pub fn asks(&self) -> bool {
        self.question.is_some()
    }

    pub fn is_text_rule(&self) -> bool {
        !matches!(self.shape, Shape::Tree(_))
    }

    /// The answer that applies a correction when none is given: "yes", or
    /// the first option that changes code.
    pub fn default_answer(&self) -> Option<String> {
        if !self.asks() {
            return None;
        }
        if self.options.is_empty() {
            return Some("yes".into());
        }
        self.options
            .iter()
            .find(|o| !o.edits.is_empty())
            .map(|o| o.answer.clone())
    }

    /// The languages a text-pattern rule has patterns for (empty for others).
    pub fn pattern_languages(&self) -> Vec<String> {
        match &self.shape {
            Shape::Table(map) => map.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }

    /// True when the rule may run on this file at all.
    pub fn applies_to(&self, file: &SourceFile) -> bool {
        if !file.role.is_source() {
            return false;
        }
        if !self.languages.is_empty()
            && !file
                .lang
                .is_some_and(|l| self.languages.iter().any(|g| g.id() == l.id()))
        {
            return false;
        }
        match &self.shape {
            Shape::Tree(_) => file.tree.is_some(),
            Shape::Pattern(_) => self.within == Within::Any || file.knows_comments(),
            Shape::Table(map) => {
                file.detection.trusted()
                    && map.contains_key(file.language_name())
                    && (self.within == Within::Any || file.knows_comments())
            }
        }
    }

    /// Find every place in `file` where this rule's shape appears and its
    /// facts hold.
    pub fn find(
        &self,
        file: &SourceFile,
        project: &Project,
        answers: &(dyn Fn(&str) -> Option<String> + Sync),
    ) -> Result<Vec<Finding>, RuleError> {
        if !self.applies_to(file) {
            return Ok(Vec::new());
        }
        let rule_err = |m: String| RuleError {
            rule: self.id.clone(),
            file: file.display_path(),
            message: m,
        };
        let mut raw: Vec<RawMatch> = Vec::new();
        let mut consider = |bindings: Bindings| -> Result<(), RuleError> {
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
            for (src, e) in &self.facts {
                if !eval_bool(e, &ctx).map_err(|er| rule_err(format!("where '{src}': {er}")))? {
                    return Ok(());
                }
            }
            raw.push(self.raw_match(&ctx).map_err(&rule_err)?);
            Ok(())
        };

        match &self.shape {
            Shape::Tree(queries) => {
                let lang = file.lang.expect("checked by applies_to");
                let Some((_, query)) = queries.iter().find(|(id, _)| *id == lang.id()) else {
                    return Ok(Vec::new());
                };
                let root = file.root().expect("checked by applies_to");
                let names = query.capture_names();
                let mut cursor = QueryCursor::new();
                // A deterministic bound on partial matches: a shape that pairs
                // every sibling with every other would otherwise take time
                // cubic in the number of siblings (a 3,600-entry dictionary
                // took minutes). Exceeding it is reported, never silent.
                cursor.set_match_limit(MATCH_LIMIT);
                let mut matches = cursor.matches(query, root, file.text.as_bytes());
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
                    consider(bindings)?;
                }
                drop(matches);
                if cursor.did_exceed_match_limit() {
                    return Err(rule_err(format!(
                        "the shape had more than {MATCH_LIMIT} partial matches at once in this file, so its matches \
                         here may be incomplete and the rule was skipped; a shape that pairs siblings with each \
                         other is quadratic (use a fact such as earlier_same_field instead)"
                    )));
                }
            }
            Shape::Pattern(_) | Shape::Table(_) => {
                let regexes: Vec<&Regex> = match &self.shape {
                    Shape::Pattern(re) => vec![re],
                    Shape::Table(map) => map
                        .get(file.language_name())
                        .map(|v| v.iter().collect())
                        .unwrap_or_default(),
                    Shape::Tree(_) => unreachable!(),
                };
                for re in regexes {
                    for caps in re.captures_iter(&file.text) {
                        let whole = caps.get(0).expect("group 0 always matches");
                        if whole.is_empty() {
                            continue;
                        }
                        let accepted = match self.within {
                            Within::Any => true,
                            w => {
                                let wanted = match w {
                                    Within::Code => SegKind::Code,
                                    Within::Comment => SegKind::Comment,
                                    _ => SegKind::String,
                                };
                                file.kind_at(whole.start()) == Some(wanted)
                            }
                        };
                        if !accepted {
                            continue;
                        }
                        let mut bindings: Bindings = BTreeMap::new();
                        bindings.insert("match".into(), Value::Spans(vec![(whole.start(), whole.end())]));
                        for name in re.capture_names().flatten() {
                            let spans = caps.name(name).map(|g| vec![(g.start(), g.end())]).unwrap_or_default();
                            bindings.insert(name.to_string(), Value::Spans(spans));
                        }
                        consider(bindings)?;
                    }
                }
            }
        }

        // The same place can be matched by several alternatives of a shape.
        let mut seen = BTreeSet::new();
        raw.retain(|r| seen.insert((r.span.start_byte, r.span.end_byte, format!("{:?}", r.edits))));
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
                    for (key, outcome) in r.edits {
                        let Some(slot) = existing.edits.iter_mut().find(|(k, _)| *k == key) else {
                            existing.edits.push((key, outcome));
                            continue;
                        };
                        let refused = match (&mut slot.1, outcome) {
                            (Ok(a), Ok(b)) => {
                                a.extend(b);
                                a.sort();
                                a.dedup();
                                None
                            }
                            (Ok(_), Err(reason)) => Some(reason),
                            (Err(_), _) => None,
                        };
                        if let Some(reason) = refused {
                            slot.1 = Err(reason);
                        }
                    }
                    continue;
                }
            }
            merged.push(r);
        }

        let mut findings = Vec::new();
        for r in merged {
            for (_, outcome) in &r.edits {
                if let Ok(edits) = outcome {
                    if let Err(e) = edit::normalize(edits) {
                        return Err(rule_err(format!("the correction produces conflicting edits: {e}")));
                    }
                }
            }
            let outcome_for = |key: &str| r.edits.iter().find(|(k, _)| k == key).map(|(_, o)| o.clone());
            let (status, edits) = match (&r.question, self.changes_code()) {
                (_, false) => (Status::Explained, Vec::new()),
                (None, true) => match outcome_for("") {
                    Some(Ok(e)) => (Status::Fixable, e),
                    Some(Err(reason)) => (Status::Refused { reason }, Vec::new()),
                    None => (Status::Explained, Vec::new()),
                },
                (Some(text), true) => {
                    let options: Vec<QuestionOption> = if self.options.is_empty() {
                        vec![
                            QuestionOption {
                                answer: "yes".into(),
                                means: "apply the correction".into(),
                                changes: true,
                                unavailable: match outcome_for("yes") {
                                    Some(Err(reason)) => Some(reason),
                                    _ => None,
                                },
                            },
                            QuestionOption {
                                answer: "no".into(),
                                means: "keep the code as it is, recorded as intentional".into(),
                                changes: false,
                                unavailable: None,
                            },
                        ]
                    } else {
                        self.options
                            .iter()
                            .map(|o| QuestionOption {
                                answer: o.answer.clone(),
                                means: o.means.clone(),
                                changes: !o.edits.is_empty(),
                                unavailable: match outcome_for(&o.answer) {
                                    Some(Err(reason)) => Some(reason),
                                    _ => None,
                                },
                            })
                            .collect()
                    };
                    let question = Question {
                        id: question_id(&self.id, &file.display_path(), &r.anchor),
                        text: text.clone(),
                        options,
                    };
                    let chosen = answers(&question.id).and_then(|a| question.option(&a).cloned());
                    match chosen {
                        None => (Status::NeedsAnswer { question }, Vec::new()),
                        Some(opt) if !opt.changes => (
                            Status::Declined {
                                answer: opt.answer.clone(),
                                question,
                            },
                            Vec::new(),
                        ),
                        Some(opt) => match outcome_for(&opt.answer) {
                            Some(Ok(e)) => (Status::Fixable, e),
                            Some(Err(reason)) => (Status::Refused { reason }, Vec::new()),
                            None => (Status::NeedsAnswer { question }, Vec::new()),
                        },
                    }
                }
            };
            let answered = r.question.is_some() && matches!(status, Status::Fixable);
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
                answered,
                edits,
            });
        }
        Ok(findings)
    }

    fn focus(&self, ctx: &EvalCtx<'_>) -> Option<(usize, usize)> {
        if let Some(name) = self.spec.focus.as_deref().and_then(capture_name) {
            return ctx
                .bindings
                .get(name)
                .and_then(|v| ctx.extent(v, "focus").ok().flatten());
        }
        // Default: the capture covering the most text.
        ctx.bindings
            .values()
            .filter_map(|v| match v {
                Value::Nodes(_) | Value::Spans(_) => ctx.extent(v, "focus").ok().flatten(),
                _ => None,
            })
            .max_by_key(|(s, e)| (e - s, usize::MAX - s))
    }

    fn raw_match(&self, ctx: &EvalCtx<'_>) -> Result<RawMatch, String> {
        let (start, end) = self.focus(ctx).ok_or("the match has no captured text to report")?;
        let span = Span::from_range(&ctx.file.text, start, end);
        let message = redact(&self.summary.render(ctx).map_err(|e| format!("summary: {e}"))?).into_owned();
        let question = match &self.question {
            Some(t) => Some(redact(&t.render(ctx).map_err(|e| format!("ask.question: {e}"))?).into_owned()),
            None => None,
        };
        let mut anchor_parts = Vec::new();
        for t in &self.anchor {
            anchor_parts.push(t.render(ctx).map_err(|e| format!("anchor: {e}"))?);
        }
        let mut evidence = Vec::new();
        for (cap, label) in &self.evidence {
            if let Some((a, b)) = ctx
                .bindings
                .get(cap)
                .and_then(|v| ctx.extent(v, "evidence").ok().flatten())
            {
                evidence.push(Evidence {
                    span: Span::from_range(&ctx.file.text, a, b),
                    label: redact(&label.render(ctx).map_err(|e| format!("evidence.label: {e}"))?).into_owned(),
                });
            }
        }
        let mut edits = Vec::new();
        if let Some(fix) = &self.fix {
            let key = if self.question.is_some() { "yes" } else { "" };
            edits.push((key.to_string(), self.build_edits(fix, ctx)?));
        }
        for o in &self.options {
            if !o.edits.is_empty() {
                edits.push((o.answer.clone(), self.build_edits(&o.edits, ctx)?));
            }
        }
        let focus_text: String = redact(&ctx.file.text[start..end])
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        Ok(RawMatch {
            span,
            message,
            question,
            evidence,
            edits,
            def_path: definition_path_at(ctx.file, start, end),
            focus_text,
            anchor: anchor_parts.join("|"),
        })
    }

    /// Build the edits of a correction. The outer error is a rule bug; the
    /// inner error is a guard refusing a correction that is unsafe here.
    fn build_edits(&self, specs: &[CompiledEdit], ctx: &EvalCtx<'_>) -> Result<Result<Vec<Edit>, String>, String> {
        let text = &ctx.file.text;
        let mut out = Vec::new();
        for (i, e) in specs.iter().enumerate() {
            let render = || -> Result<String, String> {
                e.with
                    .as_ref()
                    .map(|t| t.render(ctx).map_err(|er| format!("edit {i}: {er}")))
                    .unwrap_or_else(|| Ok(String::new()))
            };
            if e.op == EditOp::AddImport {
                let stmt = render()?;
                let Some(lang) = ctx.file.lang else {
                    return Ok(Err("imports can only be added to files with a syntax tree".into()));
                };
                match lang.add_import(ctx.file, &stmt) {
                    Ok(edit) => out.push(edit),
                    Err(reason) => return Ok(Err(reason)),
                }
                continue;
            }
            let target = e.target.as_deref().expect("compiled");
            let Some((start, end)) = ctx
                .bindings
                .get(target)
                .and_then(|v| ctx.extent(v, "edit").ok().flatten())
            else {
                return Err(format!("edit {i}: capture @{target} is empty in this match"));
            };
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
                    // blank line beside text that owns its lines cannot be
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
                    let nodes = ctx.bound_nodes(target);
                    let (Some(first), Some(last)) = (nodes.first(), nodes.last()) else {
                        return Ok(Err("delete_item needs a syntax-tree capture".to_string()));
                    };
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
        // Guard: never edit code a build cannot verify.
        for e in &out {
            let repairs = self.spec.semantics == Some(crate::model::Semantics::Repaired);
            if let Some(reason) = crate::guards::unverifiable(ctx.file, e.start, e.end, repairs) {
                return Ok(Err(reason));
            }
        }
        // Guard: a correction never silently deletes a comment.
        for e in &out {
            if e.start == e.end {
                continue;
            }
            for (cs, ce) in comments_in(ctx.file, e.start, e.end) {
                let ctext = &ctx.file.text[cs..ce];
                if !e.text.contains(ctext) {
                    return Ok(Err(format!(
                        "the correction would delete the comment `{}` on line {}",
                        ctext.trim(),
                        ctx.file.line_of(cs)
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
    /// Edits per answer: "" for a plain fix, "yes" or an option's answer.
    edits: Vec<(String, Result<Vec<Edit>, String>)>,
    def_path: String,
    focus_text: String,
    anchor: String,
}

/// `Class.method` style path of the definitions enclosing `node`.
pub fn definition_path(file: &SourceFile, node: &Node) -> String {
    let Some(lang) = file.lang else {
        return String::new();
    };
    let mut parts = Vec::new();
    let mut cur = node.parent();
    while let Some(n) = cur {
        if let Some(name) = lang.definition_name(&n, &file.text) {
            parts.push(name);
        }
        cur = n.parent();
    }
    parts.reverse();
    parts.join(".")
}

/// The definitions enclosing a byte range (empty for files without a tree).
pub fn definition_path_at(file: &SourceFile, start: usize, end: usize) -> String {
    match file.root().and_then(|r| r.descendant_for_byte_range(start, end)) {
        Some(n) => definition_path(file, &n),
        None => String::new(),
    }
}

/// FNV-1a: small, stable across platforms and versions.
/// A relative path with no `..`, root or drive: safe to join to a directory.
pub fn is_plain_relative(p: &str) -> bool {
    use std::path::Component;
    !p.is_empty()
        && !p.contains('\\')
        && std::path::Path::new(p)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

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
    /// CPU time spent in each rule, summed over files and threads, in rule order.
    pub timings: Vec<(String, std::time::Duration)>,
}

/// Run every rule on every file (or only the files in `only_files`).
/// Files are matched in parallel; the result does not depend on how.
pub fn scan<'r>(
    project: &Project,
    rules: impl IntoIterator<Item = &'r CompiledRule> + Clone,
    answers: &(dyn Fn(&str) -> Option<String> + Sync),
    only_files: Option<&BTreeSet<usize>>,
) -> Scan {
    let rules: Vec<&CompiledRule> = rules.into_iter().collect();
    let indices: Vec<usize> = (0..project.files.len())
        .filter(|i| only_files.is_none_or(|set| set.contains(i)))
        .collect();
    let per_file = crate::parallel::map(&indices, |&idx| {
        let file = &project.files[idx];
        let mut found = Vec::new();
        let mut errors = Vec::new();
        let mut times = vec![std::time::Duration::ZERO; rules.len()];
        for (i, rule) in rules.iter().enumerate() {
            let t = std::time::Instant::now();
            match rule.find(file, project, answers) {
                Ok(f) => found.extend(f),
                Err(e) => errors.push(e),
            }
            times[i] += t.elapsed();
        }
        (found, errors, times)
    });
    let mut out = Scan {
        timings: rules
            .iter()
            .map(|r| (r.id.clone(), std::time::Duration::ZERO))
            .collect(),
        ..Scan::default()
    };
    for (found, errors, times) in per_file {
        out.findings.extend(found);
        out.errors.extend(errors);
        for (slot, t) in out.timings.iter_mut().zip(times) {
            slot.1 += t;
        }
    }
    supersede(&mut out.findings);
    let equivalents: std::collections::HashMap<String, Vec<String>> = rules
        .iter()
        .filter(|r| !r.spec.equivalents.is_empty())
        .map(|r| (r.id.clone(), r.spec.equivalents.clone()))
        .collect();
    crate::suppress::apply_inline(project, &mut out.findings, &crate::suppress::today(), &equivalents);
    sort_findings(&mut out.findings);
    out
}

/// Drop findings that a correction makes moot:
///
/// * an explain-only finding at exactly the place of a finding that carries
///   a correction adds nothing the correction does not already say;
/// * a question or explanation about code that a proven correction deletes
///   (dead code, say) would ask about something that is about to disappear.
fn supersede(findings: &mut Vec<Finding>) {
    let corrected: BTreeSet<(std::path::PathBuf, usize, usize)> = findings
        .iter()
        .filter(|f| !matches!(f.status, Status::Explained))
        .map(|f| (f.file.clone(), f.span.start_byte, f.span.end_byte))
        .collect();
    let deleted: Vec<(std::path::PathBuf, usize, usize)> = findings
        .iter()
        .filter(|f| f.is_fixable())
        .flat_map(|f| {
            f.edits
                .iter()
                .filter(|e| e.text.is_empty() && e.end > e.start)
                .map(|e| (f.file.clone(), e.start, e.end))
                .collect::<Vec<_>>()
        })
        .collect();
    findings.retain(|f| {
        let same_place = (f.file.clone(), f.span.start_byte, f.span.end_byte);
        if matches!(f.status, Status::Explained) && corrected.contains(&same_place) {
            return false;
        }
        let moot = !f.is_fixable()
            && deleted
                .iter()
                .any(|(file, s, e)| *file == f.file && *s <= f.span.start_byte && f.span.end_byte <= *e);
        !moot
    });
}

pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        (a.category, &a.file, a.span.start_byte, &a.rule).cmp(&(b.category, &b.file, b.span.start_byte, &b.rule))
    });
}
