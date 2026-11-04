//! Findings in the formats CI systems and code hosts read: SARIF 2.1.0
//! (GitHub code scanning, Azure DevOps, most security dashboards), GitLab
//! Code Quality, GitHub workflow annotations and job summaries, and GitHub
//! pull-request reviews with suggested changes.
//!
//! Every format carries the finding's stable fingerprint, so a host can
//! track one finding across runs, and every excerpt is redacted.

use crate::policy::{Level, Policy};
use codemosis_core::redact::redact;
use codemosis_core::{edit, CompiledRule, Distance, Finding, Project, Status};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

pub const JSON_CHECK_SCHEMA: &str = "codemosis/check/v1";
pub const JSON_FIX_SCHEMA: &str = "codemosis/fix/v1";

/// Everything a check produced, as the CI formats need it.
pub struct CheckOutput<'a> {
    pub project: &'a Project,
    /// Every finding, including suppressed and baselined ones.
    pub findings: &'a [Finding],
    /// Fingerprints of findings accepted by the baseline, when one is used.
    pub baselined: Option<&'a BTreeSet<String>>,
    pub rules: &'a [&'a CompiledRule],
    pub policy: &'a Policy,
}

impl CheckOutput<'_> {
    fn is_baselined(&self, f: &Finding) -> bool {
        self.baselined.is_some_and(|b| b.contains(&f.fingerprint()))
    }

    /// Open findings that are not in the baseline: what a gate looks at.
    pub fn new_findings(&self) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|f| f.is_open() && !self.is_baselined(f))
            .collect()
    }

    fn level(&self, f: &Finding) -> Level {
        self.policy.level(&f.rule, f.category)
    }

    fn rule(&self, id: &str) -> Option<&CompiledRule> {
        self.rules.iter().copied().find(|r| r.id == id)
    }

    fn text_of(&self, f: &Finding) -> Option<&str> {
        self.project
            .files
            .iter()
            .find(|s| s.path == f.file)
            .map(|s| s.text.as_str())
    }
}

/// A 1-based line and a 1-based column in UTF-16 code units (what SARIF
/// consumers and editors expect) for a byte offset.
pub fn line_col_utf16(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = text[..offset].matches('\n').count() + 1;
    let col = text[line_start..offset].encode_utf16().count() + 1;
    (line, col)
}

fn region(text: Option<&str>, start: usize, end: usize, fallback: &codemosis_core::Span) -> Value {
    match text {
        Some(t) if end <= t.len() => {
            let (sl, sc) = line_col_utf16(t, start);
            let (el, ec) = line_col_utf16(t, end);
            json!({ "startLine": sl, "startColumn": sc, "endLine": el, "endColumn": ec })
        }
        _ => json!({
            "startLine": fallback.start.line,
            "startColumn": fallback.start.column,
            "endLine": fallback.end.line,
            "endColumn": fallback.end.column,
        }),
    }
}

fn uri(f: &Finding) -> String {
    f.file.to_string_lossy().replace('\\', "/")
}

fn sarif_level(l: Level) -> &'static str {
    l.as_str()
}

fn problem_severity(l: Level) -> &'static str {
    match l {
        Level::Error => "error",
        Level::Warning => "warning",
        Level::Note => "recommendation",
    }
}

/// SARIF 2.1.0.
pub fn sarif(out: &CheckOutput, root_uri: Option<&str>) -> Value {
    let mut rule_index: BTreeMap<&str, usize> = BTreeMap::new();
    let mut descriptors = Vec::new();
    for r in out.rules {
        rule_index.insert(&r.id, descriptors.len());
        let spec = &r.spec;
        let level = out.policy.level(&r.id, spec.category);
        let mut tags: Vec<String> = vec![spec.category.as_str().to_string()];
        if let Some(sem) = spec.semantics {
            tags.push(format!("semantics:{}", sem.as_str()));
        }
        tags.extend(spec.tags.iter().cloned());
        let mut security_severity = None;
        for c in &spec.cwe {
            tags.push(format!("external/cwe/{}", c.to_lowercase()));
            tags.push("security".to_string());
            security_severity = Some(match level {
                Level::Error => "7.0",
                Level::Warning => "5.0",
                Level::Note => "3.0",
            });
        }
        tags.dedup();
        let action = match (r.changes_code(), r.asks()) {
            (true, false) => "Reverse Codemosis corrects this automatically when every fact the rule needs holds.",
            (true, true) => "Reverse Codemosis asks before correcting this, because the decision is yours.",
            _ => "No correction can be proven for this; the explanation is the result.",
        };
        let mut markdown = format!("{}\n\n{action}", spec.explain.trim());
        if let Some(d) = &spec.deprecated {
            markdown.push_str(&format!("\n\n**Deprecated:** {d}"));
        }
        for reference in &spec.references {
            markdown.push_str(&format!("\n\nSee also: {reference}"));
        }
        let mut props = json!({
            "tags": tags,
            "precision": if r.changes_code() { "very-high" } else { "high" },
            "problem.severity": problem_severity(level),
        });
        if let Some(s) = security_severity {
            props["security-severity"] = json!(s);
        }
        let mut d = json!({
            "id": r.id,
            "name": spec.title,
            "shortDescription": { "text": spec.title },
            "fullDescription": { "text": spec.explain.trim() },
            "help": { "text": format!("{}\n\n{action}", spec.explain.trim()), "markdown": markdown },
            "defaultConfiguration": { "level": sarif_level(level) },
            "properties": props,
        });
        if let Some(u) = spec.references.iter().find(|u| u.starts_with("http")) {
            d["helpUri"] = json!(u);
        }
        descriptors.push(d);
    }

    let mut results = Vec::new();
    for f in out.findings {
        if matches!(f.status, Status::Declined { .. }) {
            continue;
        }
        let text = out.text_of(f);
        let mut location = json!({
            "physicalLocation": {
                "artifactLocation": { "uri": uri(f), "uriBaseId": "%SRCROOT%" },
                "region": region(text, f.span.start_byte, f.span.end_byte, &f.span),
            }
        });
        if let Some(t) = text {
            let snippet: String = t[f.span.start_byte..f.span.end_byte.min(t.len())]
                .chars()
                .take(2000)
                .collect();
            location["physicalLocation"]["region"]["snippet"] = json!({ "text": redact(&snippet) });
        }
        let related: Vec<Value> = f
            .evidence
            .iter()
            .enumerate()
            .map(|(i, e)| {
                json!({
                    "id": i + 1,
                    "message": { "text": e.label },
                    "physicalLocation": {
                        "artifactLocation": { "uri": uri(f), "uriBaseId": "%SRCROOT%" },
                        "region": region(text, e.span.start_byte, e.span.end_byte, &e.span),
                    }
                })
            })
            .collect();
        let mut props = json!({
            "category": f.category.as_str(),
            "status": f.status.label(),
            "confidence": f.confidence().as_str(),
        });
        if let Some(sem) = f.semantics {
            props["semantics"] = json!(sem.as_str());
        }
        if let Some(q) = f.question() {
            props["question"] = json!({
                "id": q.id,
                "text": q.text,
                "answers": q.options.iter().map(|o| json!({"answer": o.answer, "means": o.means})).collect::<Vec<_>>(),
            });
        }
        let mut message = f.message.clone();
        if let Status::NeedsAnswer { question } = &f.status {
            message.push_str(&format!(
                " Question {}: {} Answer with `codemosis answer {} <{}>`.",
                question.id,
                question.text,
                question.id,
                question.choices()
            ));
        }
        let mut result = json!({
            "ruleId": f.rule,
            "level": sarif_level(out.level(f)),
            "message": { "text": message },
            "locations": [location],
            "partialFingerprints": { "codemosis/v1": f.fingerprint() },
            "properties": props,
        });
        if let Some(i) = rule_index.get(f.rule.as_str()) {
            result["ruleIndex"] = json!(i);
        }
        if !related.is_empty() {
            result["relatedLocations"] = json!(related);
        }
        if out.baselined.is_some() {
            result["baselineState"] = json!(if out.is_baselined(f) { "unchanged" } else { "new" });
        }
        if let Status::Suppressed { suppression } = &f.status {
            result["suppressions"] = json!([{
                "kind": match suppression.kind {
                    codemosis_core::SuppressionKind::InSource => "inSource",
                    codemosis_core::SuppressionKind::Config => "external",
                },
                "status": "accepted",
                "justification": suppression.reason.clone().unwrap_or_default(),
            }]);
        }
        if f.is_fixable() && !f.edits.is_empty() {
            if let Some(t) = text {
                let replacements: Vec<Value> = f
                    .edits
                    .iter()
                    .map(|e| {
                        json!({
                            "deletedRegion": region(Some(t), e.start, e.end, &f.span),
                            "insertedContent": { "text": e.text },
                        })
                    })
                    .collect();
                let description = match f.semantics {
                    Some(sem) => format!("Proven correction ({}: {})", sem.as_str(), sem.describe()),
                    None => "Proven correction".to_string(),
                };
                result["fixes"] = json!([{
                    "description": { "text": description },
                    "artifactChanges": [{
                        "artifactLocation": { "uri": uri(f), "uriBaseId": "%SRCROOT%" },
                        "replacements": replacements,
                    }]
                }]);
            }
        }
        results.push(result);
    }

    let mut run = json!({
        "tool": {
            "driver": {
                "name": "Reverse Codemosis",
                "informationUri": "https://github.com/graybyrd13/reverse-codemosis",
                "version": codemosis_core::version::ENGINE_VERSION,
                "semanticVersion": codemosis_core::version::ENGINE_VERSION,
                "rules": descriptors,
            }
        },
        "results": results,
        "columnKind": "utf16CodeUnits",
        "invocations": [{ "executionSuccessful": true }],
    });
    if let Some(u) = root_uri {
        run["originalUriBaseIds"] = json!({ "%SRCROOT%": { "uri": u } });
    }
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [run],
    })
}

/// GitLab Code Quality report (a JSON array; see GitLab's "Code Quality"
/// documentation). Only open findings that are not in the baseline.
pub fn gitlab(out: &CheckOutput) -> Value {
    let items: Vec<Value> = out
        .new_findings()
        .into_iter()
        .map(|f| {
            let severity = match (out.level(f), f.category) {
                (Level::Error, codemosis_core::Category::Broken) => "critical",
                (Level::Error, _) => "major",
                (Level::Warning, _) => "minor",
                (Level::Note, _) => "info",
            };
            json!({
                "type": "issue",
                "check_name": f.rule,
                "description": format!("{} ({})", f.message, f.status.label()),
                "categories": [gitlab_category(f.category)],
                "fingerprint": f.fingerprint(),
                "severity": severity,
                "location": {
                    "path": uri(f),
                    "lines": { "begin": f.span.start.line, "end": f.span.end.line },
                },
            })
        })
        .collect();
    Value::Array(items)
}

fn gitlab_category(c: codemosis_core::Category) -> &'static str {
    match c {
        codemosis_core::Category::Broken => "Bug Risk",
        codemosis_core::Category::Contradiction => "Bug Risk",
        codemosis_core::Category::Hazard => "Security",
        codemosis_core::Category::Waste => "Performance",
        codemosis_core::Category::Simplification => "Clarity",
    }
}

fn escape_data(s: &str) -> String {
    s.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

fn escape_property(s: &str) -> String {
    escape_data(s).replace(':', "%3A").replace(',', "%2C")
}

/// GitHub Actions workflow commands: one annotation per open, new finding.
pub fn github_annotations(out: &CheckOutput) -> String {
    let mut s = String::new();
    for f in out.new_findings() {
        let cmd = match out.level(f) {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Note => "notice",
        };
        let mut message = f.message.clone();
        if let Some(q) = f.question() {
            message.push_str(&format!(
                "\n{}: {} Answer with: codemosis answer {} <{}>",
                q.id,
                q.text,
                q.id,
                q.choices()
            ));
        } else if f.is_fixable() {
            message.push_str("\nA proven correction is available: run `codemosis fix`.");
        }
        let mut props = format!(
            "file={},line={},col={}",
            escape_property(&uri(f)),
            f.span.start.line,
            f.span.start.column
        );
        if f.span.end.line != f.span.start.line {
            let _ = write!(props, ",endLine={}", f.span.end.line);
        } else {
            let _ = write!(props, ",endColumn={}", f.span.end.column);
        }
        let _ = write!(
            props,
            ",title={}",
            escape_property(&format!("{} ({})", f.title, f.rule))
        );
        let _ = writeln!(s, "::{cmd} {props}::{}", escape_data(&message));
    }
    s
}

/// A Markdown summary for a CI job page (`$GITHUB_STEP_SUMMARY`) or a
/// pull-request comment.
pub fn summary_markdown(out: &CheckOutput, distance: &Distance, link: Option<&dyn Fn(&Finding) -> String>) -> String {
    let new = out.new_findings();
    let suppressed = out
        .findings
        .iter()
        .filter(|f| matches!(f.status, Status::Suppressed { .. }))
        .count();
    let baselined = out
        .findings
        .iter()
        .filter(|f| f.is_open() && out.is_baselined(f))
        .count();
    let mut s = String::new();
    let _ = writeln!(s, "## Reverse Codemosis\n");
    if new.is_empty() {
        let _ = writeln!(
            s,
            "No new findings. The code is at its best form as far as the packs can prove.\n"
        );
    }
    let _ = writeln!(s, "| broken | contradiction | hazard | waste | simplification |");
    let _ = writeln!(s, "|---:|---:|---:|---:|---:|");
    let _ = writeln!(
        s,
        "| {} | {} | {} | {} | {} |\n",
        distance.broken, distance.contradiction, distance.hazard, distance.waste, distance.simplification
    );
    let fixable = new.iter().filter(|f| f.is_fixable()).count();
    let questions = new
        .iter()
        .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
        .count();
    let _ = writeln!(
        s,
        "**{}** new finding(s): **{fixable}** provably fixable, **{questions}** need a decision, {} explained.",
        new.len(),
        new.len() - fixable - questions
    );
    if suppressed > 0 || baselined > 0 {
        let _ = writeln!(
            s,
            "{suppressed} suppressed with a reason · {baselined} accepted by the baseline."
        );
    }
    let mut by_rule: BTreeMap<&str, usize> = BTreeMap::new();
    for f in &new {
        *by_rule.entry(f.rule.as_str()).or_default() += 1;
    }
    if !by_rule.is_empty() {
        let mut top: Vec<_> = by_rule.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let _ = writeln!(s, "\n| rule | findings |\n|---|---:|");
        for (rule, n) in top.iter().take(10) {
            let _ = writeln!(s, "| `{rule}` | {n} |");
        }
        let _ = writeln!(s, "\n<details><summary>Findings</summary>\n");
        for f in new.iter().take(100) {
            let place = format!("{}:{}", uri(f), f.span.start.line);
            let place = match link {
                Some(l) => format!("[{place}]({})", l(f)),
                None => format!("`{place}`"),
            };
            let _ = writeln!(
                s,
                "- **{}** {place} ({}): {}",
                f.rule,
                f.status.label(),
                redact(&f.message)
            );
        }
        if new.len() > 100 {
            let _ = writeln!(s, "- ... and {} more", new.len() - 100);
        }
        let _ = writeln!(s, "\n</details>");
        if fixable > 0 {
            let _ = writeln!(
                s,
                "\nRun `codemosis fix` to apply the proven corrections, each verified by the build and tests."
            );
        }
    }
    s
}

/// The whole lines a finding's edits touch, before and after the edits.
pub struct Suggestion {
    pub start_line: usize,
    pub end_line: usize,
    pub replacement: String,
}

/// The suggested change for a fixable finding, as whole lines.
pub fn suggestion(text: &str, f: &Finding) -> Option<Suggestion> {
    if !f.is_fixable() || f.edits.is_empty() {
        return None;
    }
    let edits = edit::normalize(&f.edits).ok()?;
    let first = edits.first()?.start;
    let last = edits.iter().map(|e| e.end).max()?;
    let start = text[..first].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // An edit that ends just after a line break ends on the line before it.
    let last_pos = if last > first && text.as_bytes().get(last - 1) == Some(&b'\n') {
        last - 1
    } else {
        last
    };
    let end = text[last_pos..].find('\n').map(|i| last_pos + i).unwrap_or(text.len());
    let delta: isize = edits
        .iter()
        .map(|e| e.text.len() as isize - (e.end - e.start) as isize)
        .sum();
    let after = edit::apply(text, &edits).ok()?;
    let new_end = (end as isize + delta).max(start as isize) as usize;
    let replacement = after.get(start..new_end.min(after.len()))?.to_string();
    let start_line = text[..start].matches('\n').count() + 1;
    let end_line = text[..end].matches('\n').count() + 1;
    Some(Suggestion {
        start_line,
        end_line: end_line.max(start_line),
        replacement,
    })
}

/// A GitHub pull-request review with one suggested change per fixable
/// finding, ready to POST to `/repos/{owner}/{repo}/pulls/{number}/reviews`.
pub fn github_review(out: &CheckOutput) -> Value {
    let mut comments = Vec::new();
    for f in out.new_findings() {
        let Some(text) = out.text_of(f) else { continue };
        let mut body = format!("**{}** · {}\n\n{}", f.rule, f.title, redact(&f.message));
        let (start_line, line) = match suggestion(text, f) {
            Some(sug) => {
                let semantics = f
                    .semantics
                    .map(|s| format!(" ({}: {})", s.as_str(), s.describe()))
                    .unwrap_or_default();
                let _ = write!(
                    body,
                    "\n\nProven correction{semantics}:\n\n```suggestion\n{}\n```",
                    sug.replacement.trim_end_matches('\n')
                );
                (sug.start_line, sug.end_line)
            }
            None => {
                if let Some(q) = f.question() {
                    let _ = write!(
                        body,
                        "\n\n{} — answer with `codemosis answer {} <{}>`.",
                        q.text,
                        q.id,
                        q.choices()
                    );
                }
                (f.span.start.line, f.span.start.line)
            }
        };
        let mut c = json!({ "path": uri(f), "line": line, "side": "RIGHT", "body": body });
        if start_line < line {
            c["start_line"] = json!(start_line);
            c["start_side"] = json!("RIGHT");
        }
        if let Some(rule) = out.rule(&f.rule) {
            if let Some(d) = &rule.spec.deprecated {
                c["body"] = json!(format!(
                    "{}\n\n_This rule is deprecated: {d}_",
                    c["body"].as_str().unwrap_or("")
                ));
            }
        }
        comments.push(c);
    }
    json!({
        "event": "COMMENT",
        "body": format!("Reverse Codemosis: {} finding(s) in this change.", comments.len()),
        "comments": comments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codemosis_core::{Category, Edit, Span};

    fn finding(text: &str, start: usize, end: usize, edits: Vec<Edit>) -> Finding {
        Finding {
            rule: "p/r".into(),
            title: "t".into(),
            category: Category::Waste,
            semantics: None,
            file: "a.py".into(),
            span: Span::from_range(text, start, end),
            message: "m".into(),
            evidence: vec![],
            anchor: "a".into(),
            status: Status::Fixable,
            answered: false,
            edits,
        }
    }

    #[test]
    fn utf16_columns() {
        let t = "é = 1\n😀x\n";
        assert_eq!(line_col_utf16(t, 0), (1, 1));
        assert_eq!(line_col_utf16(t, 2), (1, 2));
        assert_eq!(line_col_utf16(t, t.find('x').unwrap()), (2, 3));
    }

    #[test]
    fn suggestions_cover_whole_lines() {
        let t = "a = 1\nb = not not x\nc = 3\n";
        let s = t.find("not not x").unwrap();
        let f = finding(
            t,
            s,
            s + 9,
            vec![Edit {
                start: s,
                end: s + 9,
                text: "bool(x)".into(),
            }],
        );
        let sug = suggestion(t, &f).unwrap();
        assert_eq!((sug.start_line, sug.end_line), (2, 2));
        assert_eq!(sug.replacement, "b = bool(x)");

        let s = t.find("b = ").unwrap();
        let e = t.find("c = ").unwrap();
        let f = finding(
            t,
            s,
            e,
            vec![Edit {
                start: s,
                end: e,
                text: String::new(),
            }],
        );
        let sug = suggestion(t, &f).unwrap();
        assert_eq!((sug.start_line, sug.end_line), (2, 2));
        assert_eq!(sug.replacement, "", "deleting a line suggests an empty block");
    }

    #[test]
    fn annotation_escaping() {
        assert_eq!(escape_property("a:b,c%"), "a%3Ab%2Cc%25");
        assert_eq!(escape_data("one\ntwo"), "one%0Atwo");
    }
}
