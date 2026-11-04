//! Pack self-tests: the proof that each rule does what it claims.
//!
//! For every example the rule must match `before`, rewrite it to exactly
//! `after` when applied until it stops matching (answering its question with
//! the example's `answer`), leave `after` free of syntax errors and of further
//! corrections, and, when the language has a runner, behave as its declared
//! semantics promise:
//!
//! * `preserved`: before and after produce the same output and exit status;
//! * `corrected`: after produces the declared `output`, and before does not;
//! * `repaired`:  before fails to compile or run, after runs.
//!
//! Every counterexample must produce no finding at all.

use crate::edit;
use crate::lang::{Language, RunStatus};
use crate::model::{Edit, Semantics, Status};
use crate::pack::Pack;
use crate::project::{Project, SourceFile};
use crate::rule::CompiledRule;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail(String),
    Skip(String),
}

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub outcome: Outcome,
}

#[derive(Clone, Debug)]
pub struct RuleReport {
    pub rule: String,
    pub checks: Vec<Check>,
}

impl RuleReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| !matches!(c.outcome, Outcome::Fail(_)))
    }
}

#[derive(Clone, Debug)]
pub struct PackReport {
    pub pack: String,
    pub rules: Vec<RuleReport>,
}

impl PackReport {
    pub fn passed(&self) -> bool {
        self.rules.iter().all(RuleReport::passed)
    }

    pub fn counts(&self) -> (usize, usize, usize) {
        let mut c = (0, 0, 0);
        for r in &self.rules {
            for ch in &r.checks {
                match ch.outcome {
                    Outcome::Pass => c.0 += 1,
                    Outcome::Fail(_) => c.1 += 1,
                    Outcome::Skip(_) => c.2 += 1,
                }
            }
        }
        c
    }
}

pub struct Options {
    /// Execute examples with the language's runner.
    pub run: bool,
}

pub fn test_pack(pack: &Pack, opts: &Options) -> PackReport {
    PackReport {
        pack: pack.meta.id.clone(),
        rules: pack.rules.iter().map(|r| test_rule(r, opts)).collect(),
    }
}

/// Where an example lives: its grammar (for deep rules) and file name.
#[derive(Clone, Copy)]
struct Place<'s> {
    lang: Option<&'static dyn Language>,
    path: &'s str,
}

fn place<'s>(rule: &CompiledRule, language: Option<&str>, path: Option<&'s str>) -> Place<'s> {
    let lang = match language {
        Some(id) => rule.languages.iter().copied().find(|l| l.id() == id),
        None => rule.languages.first().copied(),
    };
    let path = path.unwrap_or_else(|| lang.map(|l| l.example_file()).unwrap_or("example.txt"));
    Place { lang, path }
}

fn single_file(at: Place, text: &str) -> Project {
    let file = match at.lang {
        Some(l) => SourceFile::with_language(at.path, l, text.to_string()),
        None => SourceFile::new(at.path, text.to_string()),
    };
    Project::in_memory(vec![file])
}

fn diff(expected: &str, actual: &str) -> String {
    similar::TextDiff::from_lines(expected, actual)
        .unified_diff()
        .header("expected", "actual")
        .to_string()
}

/// Apply every fixable finding of `rule`, answering its questions with
/// `answer`, until the rule no longer has anything to fix.
fn apply_to_fixpoint(rule: &CompiledRule, at: Place, text: &str, answer: Option<&str>) -> Result<String, String> {
    let answers = |_: &str| answer.map(|a| a.to_string());
    let mut text = text.to_string();
    for round in 0..10 {
        let project = single_file(at, &text);
        let findings = rule
            .find(&project.files[0], &project, &answers)
            .map_err(|e| e.message)?;
        if round == 0 && findings.is_empty() {
            return Err("the rule does not match `before`".to_string());
        }
        if round == 0 {
            if let Some(reason) = findings.iter().find_map(|f| match &f.status {
                Status::Refused { reason } => Some(reason.clone()),
                _ => None,
            }) {
                return Err(format!("a guard refused the correction: {reason}"));
            }
        }
        let mut chosen: Vec<Edit> = Vec::new();
        for f in findings.iter().filter(|f| f.is_fixable()) {
            if f.edits.iter().all(|e| chosen.iter().all(|c| !edit::overlaps(e, c))) {
                chosen.extend(f.edits.iter().cloned());
            }
        }
        if chosen.is_empty() {
            return Ok(text);
        }
        text = edit::apply(&text, &chosen).map_err(|e| e.to_string())?;
    }
    Err("the rule keeps matching its own output (no fixpoint after 10 rounds)".to_string())
}

fn test_rule(rule: &CompiledRule, opts: &Options) -> RuleReport {
    let mut checks = Vec::new();
    let mut push = |name: String, outcome: Outcome| checks.push(Check { name, outcome });
    let semantics = rule.spec.semantics;

    for (i, ex) in rule.spec.examples.iter().enumerate() {
        let label = ex.name.clone().unwrap_or_else(|| format!("example {}", i + 1));
        let at = place(rule, ex.language.as_deref(), ex.path.as_deref());
        let before_project = single_file(at, &ex.before);
        let file = &before_project.files[0];
        if !rule.applies_to(file) {
            push(
                format!("{label}: the rule applies to `before`"),
                Outcome::Fail(format!(
                    "the rule does not apply to {} ({}, {} tier, detected as {})",
                    at.path,
                    file.language_name(),
                    file.tier().as_str(),
                    file.detection.confidence.as_str()
                )),
            );
            continue;
        }
        if semantics != Some(Semantics::Repaired)
            && rule.spec.category != crate::model::Category::Broken
            && file.syntax_errors() > 0
        {
            push(
                format!("{label}: `before` parses"),
                Outcome::Fail("`before` has syntax errors".into()),
            );
            continue;
        }

        let Some(after) = &ex.after else {
            // Explain-only rule: it must report `before`.
            let found = rule.find(file, &before_project, &|_| None);
            push(
                format!("{label}: reports `before`"),
                match found {
                    Ok(f) if !f.is_empty() => Outcome::Pass,
                    Ok(_) => Outcome::Fail("the rule does not match `before`".into()),
                    Err(e) => Outcome::Fail(e.message),
                },
            );
            continue;
        };

        let answer = ex.answer.clone().or_else(|| rule.default_answer());
        let label = match &answer {
            Some(a) if rule.asks() => format!("{label} (answer: {a})"),
            _ => label,
        };
        match apply_to_fixpoint(rule, at, &ex.before, answer.as_deref()) {
            Ok(actual) if actual == *after => push(format!("{label}: rewrites to `after`"), Outcome::Pass),
            Ok(actual) => {
                push(
                    format!("{label}: rewrites to `after`"),
                    Outcome::Fail(format!("the result differs from `after`:\n{}", diff(after, &actual))),
                );
                continue;
            }
            Err(e) => {
                push(format!("{label}: rewrites to `after`"), Outcome::Fail(e));
                continue;
            }
        }

        let after_project = single_file(at, after);
        let after_file = &after_project.files[0];
        push(
            format!("{label}: `after` has no new syntax errors"),
            if after_file.syntax_errors() <= file.syntax_errors() {
                Outcome::Pass
            } else {
                Outcome::Fail("`after` has syntax errors `before` did not have".into())
            },
        );
        let answers = |_: &str| answer.clone();
        let again = rule
            .find(after_file, &after_project, &answers)
            .map(|f| f.iter().filter(|f| f.is_fixable()).count())
            .unwrap_or(0);
        push(
            format!("{label}: nothing left to correct in `after`"),
            if again == 0 {
                Outcome::Pass
            } else {
                Outcome::Fail(format!("the rule would still correct `after` {again} time(s)"))
            },
        );

        let name = format!("{label}: behaves as `{}`", semantics.map(|s| s.as_str()).unwrap_or("?"));
        if ex.before == *after {
            push(name, Outcome::Skip("this answer keeps the code as it is".into()));
            continue;
        }
        if !opts.run || ex.run == Some(false) {
            push(name, Outcome::Skip("not executed".into()));
            continue;
        }
        let Some(runner) = at.lang.or(file.lang).and_then(|l| l.runner()) else {
            push(name, Outcome::Skip(format!("no runner for {}", file.language_name())));
            continue;
        };
        let b = runner.run(&ex.before);
        let a = runner.run(after);
        if let RunStatus::Unavailable(why) = &a.status {
            push(name, Outcome::Skip(why.clone()));
            continue;
        }
        let describe = |o: &crate::lang::RunOutcome| match &o.status {
            RunStatus::Ok => "ran successfully".to_string(),
            RunStatus::CompileError => format!("did not compile: {}", o.error_line()),
            RunStatus::Failed(code) => format!("failed (exit {code:?}): {}", o.error_line()),
            RunStatus::TimedOut => "timed out".to_string(),
            RunStatus::Unavailable(w) => w.clone(),
        };
        let mut problems = Vec::new();
        match semantics {
            Some(Semantics::Preserved) => {
                if a.status != b.status {
                    problems.push(format!("before {} but after {}", describe(&b), describe(&a)));
                }
                if a.stdout != b.stdout {
                    problems.push(format!("output differs:\n{}", diff(&b.stdout, &a.stdout)));
                }
                if a.status != RunStatus::Ok && a.error_line() != b.error_line() {
                    problems.push(format!(
                        "errors differ: before `{}`, after `{}`",
                        b.error_line(),
                        a.error_line()
                    ));
                }
            }
            Some(Semantics::Corrected) => {
                if a.status != RunStatus::Ok {
                    problems.push(format!("after {}", describe(&a)));
                }
                if ex.output.is_none() {
                    problems.push("a `corrected` example must declare the `output` of `after`".into());
                }
                if a.stdout == b.stdout && a.status == b.status {
                    problems.push("before and after behave identically, so nothing was corrected".into());
                }
            }
            Some(Semantics::Repaired) => {
                if b.status == RunStatus::Ok {
                    problems.push("`before` runs successfully, so there was nothing to repair".into());
                }
                if a.status != RunStatus::Ok {
                    problems.push(format!("after {}", describe(&a)));
                }
            }
            None => {}
        }
        if let Some(expected) = &ex.output {
            if a.stdout != *expected {
                problems.push(format!(
                    "output of `after` differs from `output`:\n{}",
                    diff(expected, &a.stdout)
                ));
            }
        }
        if let Some(expected) = &ex.output_before {
            if b.stdout != *expected {
                problems.push(format!(
                    "output of `before` differs from `output_before`:\n{}",
                    diff(expected, &b.stdout)
                ));
            }
        }
        push(
            name,
            if problems.is_empty() {
                Outcome::Pass
            } else {
                Outcome::Fail(problems.join("\n"))
            },
        );
    }

    for (i, cx) in rule.spec.counterexamples.iter().enumerate() {
        let label = cx.name.clone().unwrap_or_else(|| format!("counterexample {}", i + 1));
        let at = place(rule, cx.language.as_deref(), cx.path.as_deref());
        let p = single_file(at, &cx.code);
        let outcome = if !rule.is_text_rule() && p.files[0].syntax_errors() > 0 {
            Outcome::Fail("the code has syntax errors".into())
        } else {
            match rule.find(&p.files[0], &p, &|_| None) {
                Ok(f) if f.is_empty() => Outcome::Pass,
                Ok(f) => Outcome::Fail(format!(
                    "the rule matches at line {}: {}",
                    f[0].span.start.line, f[0].message
                )),
                Err(e) => Outcome::Fail(e.message),
            }
        };
        push(format!("{label}: not matched"), outcome);
    }

    RuleReport {
        rule: rule.id.clone(),
        checks,
    }
}
