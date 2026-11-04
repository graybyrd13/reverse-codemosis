//! `codemosis pack docs`: Markdown reference pages generated from packs.
//!
//! Every page is generated from the rules themselves (their explanation,
//! shape, facts, question and proof examples), so the documentation cannot
//! drift from what the engine runs.

use codemosis_core::{CompiledRule, Pack};
use std::fmt::Write as _;

fn action(r: &CompiledRule) -> String {
    let sem = r.spec.semantics.map(|s| s.as_str()).unwrap_or("");
    match (r.changes_code(), r.asks(), &r.spec.ask) {
        (true, false, _) => format!("corrects ({sem})"),
        (true, true, Some(a)) if !a.options.is_empty() => format!("asks, then corrects as chosen ({sem})"),
        (true, true, _) => format!("asks, corrects on yes ({sem})"),
        _ => "explains".to_string(),
    }
}

fn fence_lang(r: &CompiledRule, path: Option<&str>) -> &'static str {
    let id = r.languages.first().map(|l| l.id()).unwrap_or("");
    let ext = path.and_then(|p| p.rsplit('.').next()).unwrap_or("");
    match (id, ext) {
        (_, "ps1") => "powershell",
        (_, "sql") => "sql",
        (_, "bat") => "batch",
        (_, "R") | (_, "r") => "r",
        (_, "yml") | (_, "yaml") => "yaml",
        ("python", _) => "python",
        ("rust", _) => "rust",
        ("javascript", _) => "javascript",
        ("typescript", _) | ("tsx", _) => "typescript",
        ("go", _) => "go",
        ("java", _) => "java",
        ("c", _) => "c",
        ("cpp", _) => "cpp",
        ("csharp", _) => "csharp",
        ("ruby", _) => "ruby",
        ("php", _) => "php",
        ("bash", _) => "bash",
        ("kotlin", _) => "kotlin",
        ("scala", _) => "scala",
        ("lua", _) => "lua",
        ("swift", _) => "swift",
        _ => "text",
    }
}

fn anchor(r: &CompiledRule) -> String {
    r.spec.id.clone()
}

/// The reference page of one pack.
pub fn pack_page(p: &Pack) -> String {
    let mut s = String::new();
    let m = &p.meta;
    let _ = writeln!(s, "# {} {}\n", m.id, m.version);
    let _ = writeln!(s, "> {}\n", m.summary);
    if let Some(d) = &m.description {
        let _ = writeln!(s, "{}\n", d.trim());
    }
    let mut facts = vec![format!("**Languages:** {}", p.target())];
    if let Some(e) = &m.engine {
        facts.push(format!("**Engine:** {e}"));
    }
    facts.push(format!("**License:** {}", m.license));
    if let Some(h) = &m.homepage {
        facts.push(format!("**Home:** <{h}>"));
    }
    let _ = writeln!(s, "{}\n", facts.join(" · "));
    if let Some(d) = &m.deprecated {
        let _ = writeln!(s, "> **Deprecated:** {d}\n");
    }
    let _ = writeln!(s, "| Rule | Category | What it does | Proof |");
    let _ = writeln!(s, "|---|---|---|---|");
    for r in &p.rules {
        let _ = writeln!(
            s,
            "| [`{}`](#{}) {} | {} | {} | {} ex. · {} counter |",
            r.spec.id,
            anchor(r),
            r.spec.title,
            r.spec.category,
            action(r),
            r.spec.examples.len(),
            r.spec.counterexamples.len()
        );
    }
    for r in &p.rules {
        s.push('\n');
        s.push_str(&rule_section(r));
    }
    s
}

fn rule_section(r: &CompiledRule) -> String {
    let spec = &r.spec;
    let mut s = String::new();
    let _ = writeln!(s, "## {}\n", spec.id);
    let _ = writeln!(s, "**{}** · `{}`\n", spec.title, r.id);
    let mut meta = vec![format!("category: {}", spec.category), format!("action: {}", action(r))];
    if !spec.cwe.is_empty() {
        meta.push(spec.cwe.join(", "));
    }
    if !spec.tags.is_empty() {
        meta.push(format!("tags: {}", spec.tags.join(", ")));
    }
    let _ = writeln!(s, "{}\n", meta.join(" · "));
    if let Some(d) = &spec.deprecated {
        let mut line = format!("> **Deprecated:** {d}");
        if let Some(by) = &spec.replaced_by {
            line.push_str(&format!(" Use `{by}` instead."));
        }
        let _ = writeln!(s, "{line}\n");
    }
    let _ = writeln!(s, "{}\n", spec.explain.trim());
    match (&spec.shape, &spec.pattern, &spec.patterns) {
        (Some(q), _, _) => {
            let _ = writeln!(s, "**Shape**\n\n```scheme\n{}\n```\n", q.trim());
        }
        (_, Some(p), _) => {
            let _ = writeln!(s, "**Pattern**\n\n```regex\n{}\n```\n", p.trim());
        }
        (_, _, Some(t)) => {
            let langs = r.pattern_languages();
            let _ = writeln!(
                s,
                "**Patterns:** one per language from the table `{t}` ({} languages: {}).\n",
                langs.len(),
                langs.join(", ")
            );
        }
        _ => {}
    }
    if !spec.facts.is_empty() {
        let _ = writeln!(s, "**Facts that must hold**\n");
        for f in &spec.facts {
            let _ = writeln!(s, "- `{f}`");
        }
        s.push('\n');
    }
    if let Some(ask) = &spec.ask {
        let _ = writeln!(s, "**Question:** {}\n", ask.question.trim());
        if ask.options.is_empty() {
            let _ = writeln!(
                s,
                "- **yes**: apply the correction\n- **no**: keep the code, recorded as intentional\n"
            );
        } else {
            for o in &ask.options {
                let _ = writeln!(s, "- **{}**: {}", o.answer, o.means);
            }
            s.push('\n');
        }
    }
    for (i, ex) in spec.examples.iter().enumerate() {
        let name = ex.name.clone().unwrap_or_else(|| format!("{}", i + 1));
        let answer = ex
            .answer
            .as_deref()
            .map(|a| format!(" (answer: {a})"))
            .unwrap_or_default();
        let _ = writeln!(s, "**Example: {name}**{answer}\n");
        match &ex.after {
            Some(after) => {
                let path = ex
                    .path
                    .as_deref()
                    .or(r.languages.first().map(|l| l.example_file()))
                    .unwrap_or("example");
                let d = codemosis_engine::session::unified_diff(path, &ex.before, after);
                let body = if d.is_empty() {
                    format!("{}(unchanged)\n", ex.before)
                } else {
                    d.lines().skip(1).map(|l| format!("{l}\n")).collect()
                };
                let _ = writeln!(s, "```diff\n{body}```\n");
            }
            None => {
                let _ = writeln!(s, "```{}\n{}```\n", fence_lang(r, ex.path.as_deref()), ex.before);
            }
        }
    }
    for (i, cx) in spec.counterexamples.iter().enumerate() {
        let name = cx.name.clone().unwrap_or_else(|| format!("{}", i + 1));
        let _ = writeln!(s, "**Counterexample: {name}** (no finding)\n");
        let _ = writeln!(s, "```{}\n{}```\n", fence_lang(r, cx.path.as_deref()), cx.code);
    }
    if !spec.references.is_empty() {
        let _ = writeln!(s, "**References**\n");
        for x in &spec.references {
            let _ = writeln!(s, "- <{x}>");
        }
        s.push('\n');
    }
    s
}

/// The index page listing every pack.
pub fn index_page(packs: &[Pack]) -> String {
    let mut s = String::from(
        "# Built-in packs\n\nGenerated by `codemosis pack docs` from the rules themselves; \
         every example on these pages is a proof that `codemosis pack test` runs.\n\n\
         | Pack | Languages | Rules | Summary |\n|---|---|---:|---|\n",
    );
    for p in packs {
        let _ = writeln!(
            s,
            "| [{}]({}.md) | {} | {} | {} |",
            p.meta.id,
            p.meta.id,
            p.target(),
            p.rules.len(),
            p.meta.summary
        );
    }
    s
}
