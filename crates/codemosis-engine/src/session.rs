//! The correction loop.
//!
//! The engine closes the distance one step at a time. A step applies the
//! corrections of one rule (as many as do not overlap) and is kept only if
//! it satisfies the acceptance contract:
//!
//! 1. every finding it targets is gone afterwards;
//! 2. it introduces no finding of a more serious category;
//! 3. every file it touches still parses, with no new syntax errors;
//! 4. the build does not regress;
//! 5. no test that passed before stops passing.
//!
//! A step that fails is rolled back. If it held several corrections, half of
//! them are tried alone and the rest come back in later rounds, so a single
//! bad correction is isolated and rejected with its evidence while the good
//! ones are kept. The most serious categories are always closed first.

use crate::oracle::{regressions, Oracle, OracleRun};
use codemosis_core::rule::scan;
use codemosis_core::{edit, Category, CompiledRule, Distance, Edit, Finding, Project, Scan, Semantics};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Options {
    /// Write corrections to disk and verify them with the oracle. When false
    /// the session plans in memory: structural guards only, nothing written.
    pub write: bool,
    /// Upper bound on attempted steps, as a guard against runaway rules.
    pub max_steps: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            write: true,
            max_steps: 500,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct StepOracle {
    pub build_ok: Option<bool>,
    pub passed_before: usize,
    pub passed_after: usize,
    pub failed_after: usize,
}

/// One accepted step.
#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub number: usize,
    pub rule: String,
    pub title: String,
    pub category: Category,
    pub semantics: Option<Semantics>,
    pub findings: Vec<Finding>,
    pub files: Vec<String>,
    pub diff: String,
    pub oracle: Option<StepOracle>,
    /// Each touched file before and after the step, exactly (unredacted),
    /// for proof certificates.
    #[serde(skip)]
    pub changes: Vec<FileChange>,
}

#[derive(Clone, Debug)]
pub struct FileChange {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// A correction that failed the acceptance contract, with the evidence.
#[derive(Clone, Debug, Serialize)]
pub struct Rejection {
    pub finding: Finding,
    pub reasons: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub wrote: bool,
    pub verified: bool,
    pub baseline: Option<OracleRun>,
    pub result: Option<OracleRun>,
    pub distance_before: Distance,
    pub distance_after: Distance,
    pub steps: Vec<Step>,
    pub rejected: Vec<Rejection>,
    /// Every finding left after the session.
    pub remaining: Vec<Finding>,
    pub rule_errors: Vec<String>,
    /// The whole change, as a unified diff.
    pub diff: String,
    pub files_changed: Vec<String>,
    pub step_limit_reached: bool,
}

pub enum Event<'e> {
    Baseline(&'e OracleRun),
    Step(&'e Step),
    Rejected(&'e Rejection),
}

pub struct Session<'s> {
    pub project: &'s mut Project,
    pub rules: Vec<&'s CompiledRule>,
    pub answers: &'s (dyn Fn(&str) -> Option<String> + Sync),
    pub oracle: Option<&'s Oracle>,
    /// The project's policy: its `[[suppress]]` entries apply to every scan.
    pub policy: Option<&'s crate::policy::Policy>,
    pub options: Options,
}

struct State {
    current: Option<OracleRun>,
    steps: Vec<Step>,
    rejected: Vec<Rejection>,
    rejected_keys: BTreeSet<String>,
    rule_errors: BTreeSet<String>,
}

/// Choose corrections that do not touch the same text.
fn non_overlapping(group: Vec<Finding>) -> Vec<Finding> {
    let mut chosen: Vec<Finding> = Vec::new();
    for f in group {
        let clash = chosen
            .iter()
            .any(|c| c.file == f.file && c.edits.iter().any(|a| f.edits.iter().any(|b| edit::overlaps(a, b))));
        if !clash {
            chosen.push(f);
        }
    }
    chosen
}

/// A git-style unified diff with three lines of context.
///
/// Hunk headers are computed by counting the lines actually emitted, so the
/// diff always applies with `git apply` and with [`crate::patch`].
pub fn unified_diff(path: &str, old: &str, new: &str) -> String {
    use similar::ChangeTag;
    use std::fmt::Write as _;
    if old == new {
        return String::new();
    }
    const CONTEXT: usize = 3;
    let diff = similar::TextDiff::from_lines(old, new);
    let changes: Vec<(char, &str)> = diff
        .iter_all_changes()
        .map(|c| {
            let tag = match c.tag() {
                ChangeTag::Equal => ' ',
                ChangeTag::Delete => '-',
                ChangeTag::Insert => '+',
            };
            (tag, c.value())
        })
        .collect();
    // Line numbers (1-based) on each side before each change.
    let mut old_no = Vec::with_capacity(changes.len());
    let mut new_no = Vec::with_capacity(changes.len());
    let (mut o, mut n) = (1usize, 1usize);
    for (tag, _) in &changes {
        old_no.push(o);
        new_no.push(n);
        match tag {
            ' ' => {
                o += 1;
                n += 1;
            }
            '-' => o += 1,
            _ => n += 1,
        }
    }
    let changed: Vec<usize> = (0..changes.len()).filter(|i| changes[*i].0 != ' ').collect();
    let mut out = format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n");
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(CONTEXT);
        let mut end = changed[k];
        while k + 1 < changed.len() && changed[k + 1] <= end + 2 * CONTEXT + 1 {
            k += 1;
            end = changed[k];
        }
        let end = (end + CONTEXT).min(changes.len() - 1);
        let hunk = &changes[start..=end];
        let old_len = hunk.iter().filter(|(t, _)| *t != '+').count();
        let new_len = hunk.iter().filter(|(t, _)| *t != '-').count();
        let side = |first: usize, len: usize| {
            let first = if len == 0 { first - 1 } else { first };
            if len == 1 {
                format!("{first}")
            } else {
                format!("{first},{len}")
            }
        };
        let _ = writeln!(
            out,
            "@@ -{} +{} @@",
            side(old_no[start], old_len),
            side(new_no[start], new_len)
        );
        for (tag, value) in hunk {
            out.push(*tag);
            out.push_str(value);
            if !value.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
        k += 1;
    }
    out
}

impl Session<'_> {
    fn rank(&self, rule: &str) -> usize {
        self.rules.iter().position(|r| r.id == rule).unwrap_or(usize::MAX)
    }

    fn scan(&self, only: Option<&BTreeSet<usize>>, st: &mut State) -> Scan {
        let mut s = scan(self.project, self.rules.iter().copied(), self.answers, only);
        if let Some(p) = self.policy {
            p.apply_suppressions(&mut s.findings, &codemosis_core::suppress::today());
        }
        st.rule_errors.extend(s.errors.iter().map(|e| e.to_string()));
        s
    }

    fn verifying(&self) -> Option<&Oracle> {
        self.oracle.filter(|o| self.options.write && o.is_configured())
    }

    pub fn run(mut self, on_event: &mut dyn FnMut(Event)) -> std::io::Result<Report> {
        let originals: Vec<String> = self.project.files.iter().map(|f| f.text.clone()).collect();
        let mut st = State {
            current: None,
            steps: Vec::new(),
            rejected: Vec::new(),
            rejected_keys: BTreeSet::new(),
            rule_errors: BTreeSet::new(),
        };
        let baseline = match self.verifying() {
            Some(o) => Some(o.run()?),
            None => None,
        };
        if let Some(b) = &baseline {
            on_event(Event::Baseline(b));
        }
        st.current = baseline.clone();

        let mut current_scan = self.scan(None, &mut st);
        let distance_before = Distance::from_findings(&current_scan.findings);
        let mut attempts = 0;
        let mut step_limit_reached = false;
        loop {
            let candidates: Vec<Finding> = current_scan
                .findings
                .iter()
                .filter(|f| f.is_fixable() && !st.rejected_keys.contains(&f.key()))
                .cloned()
                .collect();
            let Some(first) = candidates.iter().min_by_key(|f| (f.category, self.rank(&f.rule))) else {
                break;
            };
            if attempts >= self.options.max_steps {
                step_limit_reached = true;
                break;
            }
            attempts += 1;
            let rule = first.rule.clone();
            let batch = non_overlapping(candidates.into_iter().filter(|f| f.rule == rule).collect());
            self.attempt(&batch, &current_scan, &mut st, on_event)?;
            current_scan = self.scan(None, &mut st);
        }

        let mut files_changed = Vec::new();
        let mut diff = String::new();
        for (file, old) in self.project.files.iter().zip(&originals) {
            if file.text != *old {
                let path = file.display_path();
                diff.push_str(&codemosis_core::redact::redact(&unified_diff(&path, old, &file.text)));
                files_changed.push(path);
            }
        }
        Ok(Report {
            wrote: self.options.write,
            verified: self.verifying().is_some(),
            baseline,
            result: st.current,
            distance_before,
            distance_after: Distance::from_findings(&current_scan.findings),
            steps: st.steps,
            rejected: st.rejected,
            remaining: current_scan.findings,
            rule_errors: st.rule_errors.into_iter().collect(),
            diff,
            files_changed,
            step_limit_reached,
        })
    }

    fn attempt(
        &mut self,
        batch: &[Finding],
        before: &Scan,
        st: &mut State,
        on_event: &mut dyn FnMut(Event),
    ) -> std::io::Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        match self.try_step(batch, before, st)? {
            Ok(step) => {
                on_event(Event::Step(&step));
                st.steps.push(step);
            }
            Err(reasons) if batch.len() == 1 => {
                let r = Rejection {
                    finding: batch[0].clone(),
                    reasons,
                };
                st.rejected_keys.insert(r.finding.key());
                on_event(Event::Rejected(&r));
                st.rejected.push(r);
            }
            Err(_) => {
                // Isolate: try the first half alone; the rest returns later.
                self.attempt(&batch[..batch.len() / 2], before, st, on_event)?;
            }
        }
        Ok(())
    }

    fn write(&self, idx: usize) -> std::io::Result<()> {
        let f = &self.project.files[idx];
        crate::fsguard::write_confined(&self.project.root, &f.path, f.text.as_bytes())
    }

    fn restore(&mut self, snapshot: &[(usize, String)]) -> std::io::Result<()> {
        for (idx, text) in snapshot {
            self.project.set_text(*idx, text.clone());
            if self.options.write {
                self.write(*idx)?;
            }
        }
        Ok(())
    }

    /// Apply a batch and check the acceptance contract. On failure the batch
    /// is rolled back and the reasons are returned.
    fn try_step(
        &mut self,
        batch: &[Finding],
        before: &Scan,
        st: &mut State,
    ) -> std::io::Result<Result<Step, Vec<String>>> {
        codemosis_core::debug!(
            "step: trying {} correction(s) of {}",
            batch.len(),
            batch.first().map(|f| f.rule.as_str()).unwrap_or("?")
        );
        let mut per_file: BTreeMap<usize, Vec<Edit>> = BTreeMap::new();
        for f in batch {
            match self.project.index_of(&f.file) {
                Some(idx) => per_file.entry(idx).or_default().extend(f.edits.iter().cloned()),
                None => return Ok(Err(vec![format!("{} is not part of the project", f.file.display())])),
            }
        }
        let touched: BTreeSet<usize> = per_file.keys().copied().collect();
        let snapshot: Vec<(usize, String)> = touched
            .iter()
            .map(|i| (*i, self.project.files[*i].text.clone()))
            .collect();
        let errors_before: BTreeMap<usize, usize> = touched
            .iter()
            .map(|i| (*i, self.project.files[*i].syntax_errors()))
            .collect();

        let mut reasons = Vec::new();
        for (idx, edits) in &per_file {
            let old = &self.project.files[*idx].text;
            match edit::apply(old, edits) {
                Ok(new) => self.project.set_text(*idx, new),
                Err(e) => reasons.push(format!("the corrections conflict: {e}")),
            }
        }
        if reasons.is_empty() {
            for idx in &touched {
                let f = &self.project.files[*idx];
                let (b, a) = (errors_before[idx], f.syntax_errors());
                if a > b {
                    reasons.push(format!("{} would have {} new syntax error(s)", f.display_path(), a - b));
                }
            }
        }
        // Scope contract: no reference outside the edits changes its binding.
        if reasons.is_empty() {
            let repaired = batch.iter().all(|f| f.semantics == Some(Semantics::Repaired));
            for (idx, old_text) in &snapshot {
                let new_file = &self.project.files[*idx];
                let Some(lang) = new_file.lang else { continue };
                let old_file = new_file.with_text(old_text.clone());
                let edits = &per_file[idx];
                if let Some(changes) = lang.binding_changes(&old_file, new_file, edits, repaired) {
                    reasons.extend(changes.into_iter().map(|c| {
                        format!(
                            "{}: the correction changes what a name means: {c}",
                            new_file.display_path()
                        )
                    }));
                }
            }
        }
        if !reasons.is_empty() {
            self.restore(&snapshot)?;
            return Ok(Err(reasons));
        }

        // Structural contract: targets gone, nothing more serious introduced.
        let after = self.scan(Some(&touched), st);
        let touched_paths: BTreeSet<_> = touched.iter().map(|i| self.project.files[*i].path.clone()).collect();
        let before_keys: BTreeSet<String> = before
            .findings
            .iter()
            .filter(|f| touched_paths.contains(&f.file))
            .map(|f| f.key())
            .collect();
        for f in batch {
            if after.findings.iter().any(|g| g.key() == f.key()) {
                reasons.push(format!(
                    "{} still matches after its own correction at {}:{} (a defect in the rule)",
                    f.rule,
                    f.file.display(),
                    f.span.start.line
                ));
            }
        }
        let worst = batch.iter().map(|f| f.category).min().expect("non-empty batch");
        for g in &after.findings {
            if g.category < worst && !before_keys.contains(&g.key()) && g.is_open() {
                reasons.push(format!(
                    "the correction would introduce a {} finding at {}:{}: {}",
                    g.category,
                    g.file.display(),
                    g.span.start.line,
                    g.message
                ));
            }
        }
        if !reasons.is_empty() {
            codemosis_core::debug!("step: refused: {}", reasons.join("; "));
            self.restore(&snapshot)?;
            return Ok(Err(reasons));
        }

        // Behavioural contract: the project's own build and tests.
        if self.options.write {
            for idx in &touched {
                self.write(*idx)?;
            }
        }
        let mut step_oracle = None;
        if let Some(oracle) = self.verifying() {
            let run = oracle.run()?;
            let prev = st.current.as_ref().expect("baseline exists when verifying");
            let regs = regressions(prev, &run);
            if !regs.is_empty() {
                self.restore(&snapshot)?;
                codemosis_core::debug!("step: rolled back by the oracle: {}", regs.join("; "));
                return Ok(Err(regs));
            }
            step_oracle = Some(StepOracle {
                build_ok: run.build_ok(),
                passed_before: prev.passed(),
                passed_after: run.passed(),
                failed_after: run.failed(),
            });
            st.current = Some(run);
        }

        let mut diff = String::new();
        let mut files = Vec::new();
        let mut changes = Vec::new();
        for (idx, old) in &snapshot {
            let f = &self.project.files[*idx];
            diff.push_str(&codemosis_core::redact::redact(&unified_diff(
                &f.display_path(),
                old,
                &f.text,
            )));
            files.push(f.display_path());
            changes.push(FileChange {
                path: f.display_path(),
                before: old.clone(),
                after: f.text.clone(),
            });
        }
        let first = &batch[0];
        Ok(Ok(Step {
            number: st.steps.len() + 1,
            rule: first.rule.clone(),
            title: first.title.clone(),
            category: first.category,
            semantics: first.semantics,
            findings: batch.to_vec(),
            files,
            diff,
            oracle: step_oracle,
            changes,
        }))
    }
}
