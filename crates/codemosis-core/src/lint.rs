//! `codemosis pack lint`: problems a pack can have that still let it load.
//!
//! Loading a pack already rejects everything that would make a rule wrong
//! or unrunnable. The linter looks for what makes a rule weak: proofs that
//! prove little, captures nobody reads, tables nobody uses, explanations too
//! thin to satisfy a reviewer, two rules that match the same shape.

use crate::model::{Category, Semantics};
use crate::pack::Pack;
use crate::rule::CompiledRule;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Note,
    Warning,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lint {
    pub severity: Severity,
    /// `pack` or `pack/rule`.
    pub at: String,
    /// A short stable code, like `unused-capture`.
    pub code: &'static str,
    pub message: String,
}

fn words(re: &Regex, text: &str) -> BTreeSet<String> {
    re.captures_iter(text).map(|c| c[1].to_string()).collect()
}

/// Every lint for a pack, most severe first.
pub fn lint_pack(pack: &Pack) -> Vec<Lint> {
    let mut out = Vec::new();
    let at = pack.meta.id.clone();
    let mut push = |severity, at: &str, code, message: String| {
        out.push(Lint {
            severity,
            at: at.to_string(),
            code,
            message,
        })
    };
    if pack.meta.engine.is_none() {
        push(
            Severity::Warning,
            &at,
            "no-engine-range",
            "pack.toml has no `engine` requirement; add one (like \"^0.1\") so an incompatible engine refuses the pack instead of misreading it".into(),
        );
    }
    if pack.meta.homepage.is_none() {
        push(
            Severity::Note,
            &at,
            "no-homepage",
            "pack.toml has no homepage, so users cannot find where to report a wrong rule".into(),
        );
    }
    if pack.meta.description.is_none() {
        push(
            Severity::Note,
            &at,
            "no-description",
            "pack.toml has no description".into(),
        );
    }

    // Tables nobody reads.
    let mut referenced_tables = BTreeSet::new();
    let table_ref = Regex::new(r#"(?:in_table|table|lookup)\(\s*['"]([^'"]+)['"]"#).expect("valid regex");
    for r in &pack.rules {
        let s = &r.spec;
        let mut texts: Vec<String> = s.facts.clone();
        texts.extend(s.lets.values().filter_map(|v| v.as_str().map(String::from)));
        for text in &texts {
            referenced_tables.extend(words(&table_ref, text));
        }
        if let Some(p) = &s.patterns {
            referenced_tables.insert(p.clone());
        }
    }
    for name in pack.tables.tables.keys() {
        if !referenced_tables.contains(name) {
            push(
                Severity::Warning,
                &at,
                "unused-table",
                format!(
                    "table '{name}' is never read by a rule; every entry is a claim to review, so remove it or use it"
                ),
            );
        }
    }

    // Two rules with the same shape fight over the same code.
    let mut shapes: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for r in &pack.rules {
        if let Some(shape) = &r.spec.shape {
            let normal: String = shape.split_whitespace().collect::<Vec<_>>().join(" ");
            shapes.entry(normal).or_default().push(&r.id);
        }
    }
    for ids in shapes.values().filter(|ids| ids.len() > 1) {
        if pack
            .rules
            .iter()
            .filter(|r| ids.contains(&r.id.as_str()))
            .all(|r| r.spec.facts.is_empty())
        {
            push(
                Severity::Warning,
                &at,
                "duplicate-shape",
                format!(
                    "rules {} have the same shape and no facts to tell them apart",
                    ids.join(", ")
                ),
            );
        }
    }

    for r in &pack.rules {
        out.extend(lint_rule(r));
    }
    out.sort_by(|a, b| b.severity.cmp(&a.severity).then(a.at.cmp(&b.at)));
    out
}

/// The lints of one rule.
pub fn lint_rule(rule: &CompiledRule) -> Vec<Lint> {
    let s = &rule.spec;
    let mut out = Vec::new();
    let mut push = |severity, code, message: String| {
        out.push(Lint {
            severity,
            at: rule.id.clone(),
            code,
            message,
        })
    };

    // Captures defined but never read.
    let shape_capture = Regex::new(r"@([A-Za-z_][A-Za-z0-9_.]*)").expect("valid regex");
    let use_capture = Regex::new(r"@([A-Za-z_][A-Za-z0-9_]*)").expect("valid regex");
    let hole = Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)").expect("valid regex");
    let mut defined: BTreeSet<String> = s
        .shape
        .as_deref()
        .map(|q| {
            // Captures used only by query predicates count as read.
            let predicate = Regex::new(r"\(#[a-z-]+\?[^)]*\)").expect("valid regex");
            let in_predicates = words(
                &shape_capture,
                &predicate.find_iter(q).map(|m| m.as_str()).collect::<String>(),
            );
            words(&shape_capture, q)
                .into_iter()
                .filter(|c| !in_predicates.contains(c) && !c.starts_with('_'))
                .collect()
        })
        .unwrap_or_default();
    if let Some(p) = &s.pattern {
        let named = Regex::new(r"\(\?P?<([A-Za-z_][A-Za-z0-9_]*)>").expect("valid regex");
        defined.extend(words(&named, p));
    }
    let mut used_texts: Vec<String> = Vec::new();
    used_texts.extend(s.facts.iter().cloned());
    used_texts.extend(s.lets.values().filter_map(|v| v.as_str().map(String::from)));
    used_texts.push(s.summary.clone());
    used_texts.extend(s.focus.iter().cloned());
    used_texts.extend(s.anchor.iter().cloned());
    for e in &s.evidence {
        used_texts.push(e.at.clone());
        used_texts.push(e.label.clone());
    }
    let mut edit_texts = Vec::new();
    if let Some(fix) = &s.fix {
        edit_texts.extend(fix.edits.iter().cloned());
    }
    if let Some(ask) = &s.ask {
        used_texts.push(ask.question.clone());
        for o in &ask.options {
            edit_texts.extend(o.edits.iter().cloned());
        }
    }
    for e in &edit_texts {
        for v in [
            &e.replace,
            &e.delete,
            &e.delete_lines,
            &e.delete_item,
            &e.insert_before,
            &e.insert_after,
            &e.insert_lines_before,
            &e.insert_lines_after,
            &e.with,
            &e.add_import,
        ]
        .into_iter()
        .flatten()
        {
            used_texts.push(v.clone());
        }
    }
    let mut used = BTreeSet::new();
    for t in &used_texts {
        used.extend(words(&use_capture, t));
        used.extend(words(&hole, t));
    }
    // `match` is implicit in text rules; the focus defaults to the widest capture.
    used.insert("match".to_string());
    let lets: BTreeSet<String> = s.lets.keys().cloned().collect();
    for c in defined.difference(&used) {
        push(
            Severity::Warning,
            "unused-capture",
            format!("@{c} is captured but never read; a capture nobody reads often means a forgotten fact (prefix it with _ if it is only structure)"),
        );
    }
    for l in lets.difference(&used) {
        push(
            Severity::Warning,
            "unused-let",
            format!("let value '{l}' is never used"),
        );
    }

    // Proofs that prove little.
    let changes = rule.changes_code();
    if changes && s.examples.iter().all(|e| e.run == Some(false)) {
        push(
            Severity::Warning,
            "unproven-semantics",
            format!(
                "every example has run = false, so the claim semantics = \"{}\" is never checked by running code",
                s.semantics.map(|x| x.as_str()).unwrap_or("?")
            ),
        );
    }
    if changes && s.counterexamples.len() < 2 && !s.facts.is_empty() {
        push(
            Severity::Note,
            "few-counterexamples",
            format!(
                "{} where-fact(s) but {} counterexample; each fact should have a counterexample that needs it (see `codemosis pack test --mutations`)",
                s.facts.len(),
                s.counterexamples.len()
            ),
        );
    }
    if s.semantics == Some(Semantics::Corrected) && s.examples.iter().all(|e| e.output.is_none()) {
        push(
            Severity::Warning,
            "corrected-without-output",
            "a `corrected` rule should state the corrected output of at least one example".into(),
        );
    }
    if s.examples.len() > 1 && s.examples.iter().any(|e| e.name.is_none()) {
        push(
            Severity::Note,
            "unnamed-example",
            "name each example so a failing check says which case broke".into(),
        );
    }

    // Explanations and questions.
    if s.explain.trim().len() < 120 {
        push(
            Severity::Warning,
            "thin-explanation",
            "the explanation is under 120 characters; say what is wrong, why, and why the correction is safe (or why there is none)".into(),
        );
    }
    if let Some(ask) = &s.ask {
        if !ask.options.is_empty() && ask.options.iter().all(|o| !o.edits.is_empty()) {
            push(
                Severity::Warning,
                "no-keep-option",
                "every option of the question changes code; offer one that keeps it, so a person can say the code is intentional".into(),
            );
        }
        if !ask.question.trim_end().ends_with('?') {
            push(Severity::Note, "question-mark", "a question should end with '?'".into());
        }
    }
    if s.references.is_empty() && matches!(s.category, Category::Hazard | Category::Contradiction) {
        push(
            Severity::Note,
            "no-references",
            "a hazard or contradiction rule should cite documentation backing it (`references`)".into(),
        );
    }
    if s.deprecated.is_some() && s.replaced_by.is_none() {
        push(
            Severity::Note,
            "deprecated-without-replacement",
            "say which rule replaces this one (`replaced_by`), or that none does".into(),
        );
    }
    out
}
