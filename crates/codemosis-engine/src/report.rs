//! Human-readable and machine-readable reports.

use crate::oracle::OracleRun;
use crate::session::{Report, Step};
use codemosis_core::{Category, Distance, Finding, Pack, Project, Status};
use std::fmt::Write as _;

/// ANSI styling that switches itself off.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub color: bool,
}

impl Style {
    fn paint(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    pub fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.paint("31", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.paint("32", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.paint("33", s)
    }
    pub fn blue(&self, s: &str) -> String {
        self.paint("34", s)
    }
    pub fn cyan(&self, s: &str) -> String {
        self.paint("36", s)
    }

    pub fn category(&self, c: Category) -> String {
        let label = format!("{:<14}", c.as_str());
        match c {
            Category::Broken => self.red(&label),
            Category::Contradiction => self.yellow(&label),
            Category::Hazard => self.paint("35", &label),
            Category::Waste => self.blue(&label),
            Category::Simplification => self.cyan(&label),
        }
    }
}

fn source_line(project: &Project, f: &Finding, line: usize) -> Option<String> {
    let file = project.files.iter().find(|s| s.path == f.file)?;
    file.text.lines().nth(line.checked_sub(1)?).map(|s| s.to_string())
}

/// A finding, with the line it points at.
pub fn finding(f: &Finding, project: &Project, s: Style) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} {}  {}",
        s.category(f.category),
        s.bold(&f.rule),
        s.dim(&f.title)
    );
    let _ = writeln!(
        out,
        "  {} {}:{}:{}",
        s.blue("-->"),
        f.file.display(),
        f.span.start.line,
        f.span.start.column
    );
    if let Some(line) = source_line(project, f, f.span.start.line) {
        let num = f.span.start.line.to_string();
        let pad = " ".repeat(num.len());
        let start = f.span.start.column.saturating_sub(1).min(line.len());
        let end = if f.span.end.line == f.span.start.line {
            f.span.end.column.saturating_sub(1).min(line.len())
        } else {
            line.len()
        };
        let width = line[..start].chars().count();
        let carets = "^".repeat(line[start..end.max(start)].chars().count().max(1));
        let _ = writeln!(out, "  {pad} {}", s.blue("|"));
        let _ = writeln!(out, "  {} {} {}", s.blue(&num), s.blue("|"), line);
        let _ = writeln!(
            out,
            "  {pad} {} {}{}",
            s.blue("|"),
            " ".repeat(width),
            s.yellow(&carets)
        );
    }
    let _ = writeln!(out, "  {} {}", s.blue("="), f.message);
    for ev in &f.evidence {
        let _ = writeln!(
            out,
            "  {} {}:{}:{} {}",
            s.blue("="),
            f.file.display(),
            ev.span.start.line,
            ev.span.start.column,
            ev.label
        );
    }
    let status = match &f.status {
        Status::Fixable => match f.semantics {
            Some(sem) => format!(
                "{} ({}: {})",
                s.green("proven correction available"),
                sem.as_str(),
                sem.describe()
            ),
            None => s.green("proven correction available"),
        },
        Status::NeedsAnswer { question } => format!(
            "{} {}\n    {}\n    answer with: codemosis answer {} yes   (or no)",
            s.yellow("question"),
            s.bold(&question.id),
            question.text,
            question.id
        ),
        Status::Declined { question } => format!("kept as intentional (you answered no to {})", question.id),
        Status::Explained => "no correction can be proven here; the explanation is the result".to_string(),
        Status::Refused { reason } => format!("{} {}", s.red("correction refused:"), reason),
    };
    let _ = writeln!(out, "  {} {}", s.blue("="), status);
    out
}

pub fn distance(d: &Distance, s: Style) -> String {
    Category::ALL
        .iter()
        .map(|c| {
            let n = d.get(*c);
            let text = format!("{} {}", c.as_str(), n);
            if n == 0 {
                s.dim(&text)
            } else {
                text
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn oracle_summary(run: &OracleRun, s: Style) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(b) = &run.build {
        rows.push((
            "build".to_string(),
            if b.ok {
                s.green("ok")
            } else {
                s.red(&format!(
                    "failed ({} error{})",
                    b.errors,
                    if b.errors == 1 { "" } else { "s" }
                ))
            },
        ));
    }
    if let Some(t) = &run.tests {
        let passed = run.passed();
        let failed = run.failed();
        let text = if t.results.is_empty() {
            if t.ok {
                s.green("passed")
            } else {
                s.red("failed")
            }
        } else {
            let f = format!("{failed} failed");
            format!(
                "{} · {}",
                s.green(&format!("{passed} passed")),
                if failed > 0 { s.red(&f) } else { s.dim(&f) }
            )
        };
        rows.push(("tests".to_string(), text));
    }
    rows
}

pub fn packs_line(packs: &[&Pack]) -> String {
    packs
        .iter()
        .map(|p| format!("{} {} ({} rules)", p.meta.id, p.meta.version, p.rules.len()))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn step_line(step: &Step, s: Style) -> String {
    let count = step.findings.len();
    let what = if count == 1 {
        format!(
            "{}:{}",
            step.findings[0].file.display(),
            step.findings[0].span.start.line
        )
    } else {
        format!("{} corrections in {}", count, step.files.join(", "))
    };
    let oracle = match &step.oracle {
        Some(o) if o.passed_after != o.passed_before => {
            format!("  tests {} → {} passing", o.passed_before, o.passed_after)
        }
        Some(_) => "  build and tests unchanged".to_string(),
        None => String::new(),
    };
    format!(
        "  {} {:>2}  {} {}{}",
        s.green("✓"),
        step.number,
        s.bold(&step.rule),
        what,
        s.dim(&oracle)
    )
}

pub struct Header<'a> {
    pub root: &'a str,
    pub packs: &'a [&'a Pack],
    pub build: Option<&'a str>,
    pub test: Option<&'a str>,
    pub detected: bool,
}

pub fn header(h: &Header, s: Style) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}",
        s.bold(&format!("Reverse Codemosis {}", env!("CARGO_PKG_VERSION")))
    );
    let _ = writeln!(out, "  project   {}", h.root);
    let _ = writeln!(out, "  packs     {}", packs_line(h.packs));
    match (h.build, h.test) {
        (None, None) => {
            let _ = writeln!(
                out,
                "  oracle    {}",
                s.yellow("none: corrections are checked by their rules' proofs only")
            );
        }
        (b, t) => {
            let note = if h.detected {
                s.dim("  (detected; set [oracle] in codemosis.toml to change)")
            } else {
                String::new()
            };
            let _ = writeln!(out, "  oracle    build  {}{}", b.unwrap_or("-"), note);
            let _ = writeln!(out, "            test   {}", t.unwrap_or("-"));
        }
    }
    out
}

/// The terminal report of a `fix` session.
pub fn session_text(r: &Report, project: &Project, s: Style, show_diff: bool) -> String {
    let mut out = String::new();
    if let Some(b) = &r.baseline {
        let _ = writeln!(out, "\n{}", s.bold("Baseline"));
        for (k, v) in oracle_summary(b, s) {
            let _ = writeln!(out, "  {k:<9} {v}");
        }
    } else {
        let _ = writeln!(out);
    }
    let _ = writeln!(out, "  {:<9} {}", "distance", distance(&r.distance_before, s));

    let _ = writeln!(
        out,
        "\n{}",
        s.bold(if r.wrote {
            "Corrections"
        } else {
            "Planned corrections (dry run, nothing written)"
        })
    );
    if r.steps.is_empty() && r.rejected.is_empty() {
        let _ = writeln!(out, "  {}", s.dim("none: nothing here can be corrected by proof"));
    }
    for step in &r.steps {
        let _ = writeln!(out, "{}", step_line(step, s));
    }
    for rej in &r.rejected {
        let _ = writeln!(
            out,
            "  {}     {} {}:{}  {}",
            s.red("✗"),
            s.bold(&rej.finding.rule),
            rej.finding.file.display(),
            rej.finding.span.start.line,
            s.red("rejected by verification")
        );
        for reason in &rej.reasons {
            for line in reason.lines() {
                let _ = writeln!(out, "          {}", s.dim(line));
            }
        }
    }

    let _ = writeln!(out, "\n{}", s.bold("Result"));
    if let Some(res) = &r.result {
        for (k, v) in oracle_summary(res, s) {
            let _ = writeln!(out, "  {k:<9} {v}");
        }
    }
    let _ = writeln!(out, "  {:<9} {}", "distance", distance(&r.distance_after, s));
    let applied: usize = r.steps.iter().map(|st| st.findings.len()).sum();
    let verb = if r.wrote { "applied" } else { "planned" };
    let _ = writeln!(
        out,
        "  {:<9} {} correction{} in {} file{}{}",
        verb,
        applied,
        if applied == 1 { "" } else { "s" },
        r.files_changed.len(),
        if r.files_changed.len() == 1 { "" } else { "s" },
        if r.wrote && applied > 0 {
            "; review them with `git diff`"
        } else {
            ""
        }
    );
    if r.wrote && !r.verified && applied > 0 {
        let _ = writeln!(
            out,
            "  {}",
            s.yellow("unverified: no build or test command was configured")
        );
    }
    if r.step_limit_reached {
        let _ = writeln!(
            out,
            "  {}",
            s.yellow("stopped at the step limit; run again to continue")
        );
    }

    let questions: Vec<&Finding> = r
        .remaining
        .iter()
        .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
        .collect();
    let explained: Vec<&Finding> = r
        .remaining
        .iter()
        .filter(|f| matches!(f.status, Status::Explained | Status::Refused { .. }))
        .collect();
    if !questions.is_empty() {
        let _ = writeln!(
            out,
            "\n{}",
            s.bold(&format!("Needs your decision ({})", questions.len()))
        );
        for f in questions {
            let _ = writeln!(out, "{}", finding(f, project, s));
        }
    }
    if !explained.is_empty() {
        let _ = writeln!(
            out,
            "\n{}",
            s.bold(&format!("Explained, not corrected ({})", explained.len()))
        );
        for f in explained {
            let _ = writeln!(out, "{}", finding(f, project, s));
        }
    }
    if !r.rule_errors.is_empty() {
        let _ = writeln!(
            out,
            "\n{}",
            s.red(&format!(
                "Rule errors ({}): these rules were skipped where they failed",
                r.rule_errors.len()
            ))
        );
        for e in &r.rule_errors {
            let _ = writeln!(out, "  {e}");
        }
    }
    if show_diff && !r.diff.is_empty() {
        let _ = writeln!(out, "\n{}\n{}", s.bold("Diff"), r.diff);
    }
    out
}

/// A Markdown report of a session, suitable for a pull request.
pub fn session_markdown(r: &Report, root: &str, packs: &[&Pack]) -> String {
    let s = Style { color: false };
    let mut out = String::new();
    let _ = writeln!(out, "# Reverse Codemosis report\n");
    let _ = writeln!(out, "- Project: `{root}`");
    let _ = writeln!(out, "- Packs: {}", packs_line(packs));
    let _ = writeln!(
        out,
        "- Verified by build and tests: {}\n",
        if r.verified { "yes" } else { "no" }
    );
    let _ = writeln!(out, "| | Before | After |\n|---|---|---|");
    if let (Some(b), Some(a)) = (&r.baseline, &r.result) {
        let before = oracle_summary(b, s);
        let after = oracle_summary(a, s);
        for ((k, v1), (_, v2)) in before.iter().zip(after.iter()) {
            let _ = writeln!(out, "| {k} | {v1} | {v2} |");
        }
    }
    for c in Category::ALL {
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            c.as_str(),
            r.distance_before.get(c),
            r.distance_after.get(c)
        );
    }
    let _ = writeln!(out, "\n## Corrections ({})\n", r.steps.len());
    for step in &r.steps {
        let sem = step
            .semantics
            .map(|x| format!(" — {}: {}", x.as_str(), x.describe()))
            .unwrap_or_default();
        let _ = writeln!(out, "{}. **{}** ({}){}", step.number, step.rule, step.title, sem);
        for f in &step.findings {
            let _ = writeln!(out, "   - `{}:{}` {}", f.file.display(), f.span.start.line, f.message);
        }
    }
    if !r.rejected.is_empty() {
        let _ = writeln!(out, "\n## Rejected by verification ({})\n", r.rejected.len());
        for rej in &r.rejected {
            let _ = writeln!(
                out,
                "- **{}** at `{}:{}`: {}",
                rej.finding.rule,
                rej.finding.file.display(),
                rej.finding.span.start.line,
                rej.reasons.join("; ")
            );
        }
    }
    let questions: Vec<&Finding> = r
        .remaining
        .iter()
        .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
        .collect();
    if !questions.is_empty() {
        let _ = writeln!(out, "\n## Needs a decision ({})\n", questions.len());
        for f in questions {
            if let Some(q) = f.question() {
                let _ = writeln!(
                    out,
                    "- `{}` at `{}:{}`: {}  \n  {}  \n  Answer with `codemosis answer {} yes` or `no`.",
                    q.id,
                    f.file.display(),
                    f.span.start.line,
                    f.message,
                    q.text,
                    q.id
                );
            }
        }
    }
    let explained: Vec<&Finding> = r
        .remaining
        .iter()
        .filter(|f| matches!(f.status, Status::Explained | Status::Refused { .. }))
        .collect();
    if !explained.is_empty() {
        let _ = writeln!(out, "\n## Explained, not corrected ({})\n", explained.len());
        for f in explained {
            let _ = writeln!(
                out,
                "- **{}** at `{}:{}`: {}",
                f.rule,
                f.file.display(),
                f.span.start.line,
                f.message
            );
        }
    }
    if !r.diff.is_empty() {
        let _ = writeln!(out, "\n## Diff\n\n```diff\n{}```", r.diff);
    }
    out
}
