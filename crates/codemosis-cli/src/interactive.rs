//! Asking: the open questions, one at a time, with the evidence on screen.
//! Answers are recorded in the ledger as soon as they are given, so stopping
//! halfway keeps what was decided.

use anyhow::{anyhow, Result};
use codemosis_core::{Finding, Status};
use codemosis_engine::answers::Answer;
use codemosis_engine::report::{self, Style};
use codemosis_engine::{enabled_rules, Workspace};
use std::io::{BufRead, Write};

/// Ask every open question and record the answers. Returns how many were recorded.
pub fn ask(
    ws: &mut Workspace,
    only: &[String],
    s: Style,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<usize> {
    let questions: Vec<Finding> = {
        let rules = enabled_rules(&ws.packs, &ws.config, &ws.project, only);
        let answers = |id: &str| ws.answers.get(id);
        codemosis_core::scan(&ws.project, rules, &answers, None)
            .findings
            .into_iter()
            .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
            .collect()
    };
    if questions.is_empty() {
        writeln!(out, "No open questions: nothing here needs a decision from you.")?;
        return Ok(0);
    }
    let n = questions.len();
    writeln!(
        out,
        "{}",
        s.bold(&format!(
            "{n} question{} need{} a decision only you can make.",
            if n == 1 { "" } else { "s" },
            if n == 1 { "s" } else { "" }
        ))
    )?;
    writeln!(
        out,
        "{}",
        s.dim("Answer with one of the choices shown, or skip, or quit. Answers are saved as you go.")
    )?;

    let mut recorded = 0;
    'questions: for (i, f) in questions.iter().enumerate() {
        let q = f.question().expect("filtered to questions");
        writeln!(out, "\n{}", s.bold(&format!("[{}/{n}]", i + 1)))?;
        write!(out, "{}", report::finding_with(f, &ws.project, s, false))?;
        writeln!(out, "  {}", s.yellow(&q.text))?;
        for o in &q.options {
            let note = match &o.unavailable {
                Some(why) => format!("  {}", s.dim(&format!("(not possible here: {why})"))),
                None => String::new(),
            };
            writeln!(out, "    {:<8} {}{}", s.bold(&o.answer), o.means, note)?;
        }
        loop {
            write!(out, "  {} [{}/skip/quit] ", s.bold("answer"), q.choices())?;
            out.flush()?;
            let mut line = String::new();
            if input.read_line(&mut line)? == 0 {
                writeln!(out)?;
                break 'questions;
            }
            let a = line.trim().to_lowercase();
            match a.as_str() {
                "" => continue,
                "s" | "skip" => {
                    writeln!(out, "  {}", s.dim("skipped"))?;
                    continue 'questions;
                }
                "q" | "quit" => break 'questions,
                _ => {}
            }
            let Some(option) = q.option(&a) else {
                writeln!(out, "  please answer {} (or skip, or quit)", q.choices())?;
                continue;
            };
            if let Some(why) = &option.unavailable {
                writeln!(out, "  {} is not possible here: {why}", option.answer)?;
                continue;
            }
            ws.answers.set(Answer {
                id: q.id.clone(),
                rule: f.rule.clone(),
                location: format!("{}:{}", f.file.display(), f.span.start.line),
                question: q.text.clone(),
                answer: option.answer.clone(),
                note: None,
            });
            ws.answers.save(&ws.root).map_err(|e| anyhow!(e))?;
            recorded += 1;
            let what = if option.changes {
                "recorded; `codemosis fix` will apply it"
            } else {
                "recorded; the code stays as it is"
            };
            writeln!(out, "  {}", s.green(what))?;
            continue 'questions;
        }
    }
    writeln!(
        out,
        "\n{} answer{} recorded in {}.",
        recorded,
        if recorded == 1 { "" } else { "s" },
        codemosis_engine::answers::FILE
    )?;
    Ok(recorded)
}
