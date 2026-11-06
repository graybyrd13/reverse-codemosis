//! `codemosis map` and `codemosis languages`: what the engine understands,
//! made visible.

use anyhow::{bail, Result};
use codemosis_core::linguist::{self, Confidence, Kind};
use codemosis_core::{lang, lex, Status, Tier};
use codemosis_engine::report::{self, Style};
use codemosis_engine::{enabled_rules, Workspace};
use std::collections::BTreeMap;
use std::fmt::Write as _;

struct LangStats {
    name: &'static str,
    kind: Option<Kind>,
    color: Option<&'static str>,
    files: usize,
    bytes: usize,
    tier: Tier,
    grammar: Option<&'static str>,
    rules: usize,
    /// Files in a dialect the grammar does not read (zsh), by dialect.
    dialects: BTreeMap<&'static str, usize>,
}

fn hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((p(0)?, p(2)?, p(4)?))
}

fn paint_rgb(s: Style, color: Option<&str>, text: &str) -> String {
    match (s.color, color.and_then(hex_rgb)) {
        (true, Some((r, g, b))) => format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"),
        _ => text.to_string(),
    }
}

fn tier_label(t: Tier) -> &'static str {
    match t {
        Tier::Deep => "deep: syntax tree and facts",
        Tier::Lexical => "lexical: code, comments and strings",
        Tier::Recognized => "recognized: universal rules only",
    }
}

fn pretty_kind(kind: &str) -> String {
    let words = kind
        .trim_end_matches("_declaration")
        .trim_end_matches("_definition")
        .trim_end_matches("_item")
        .trim_end_matches("_specifier")
        .replace('_', " ");
    if words.ends_with('s') {
        format!("{words}es")
    } else {
        format!("{words}s")
    }
}

const ECOSYSTEM_MARKERS: &[(&str, &str)] = &[
    ("Cargo.toml", "Rust (Cargo)"),
    ("package.json", "JavaScript/TypeScript (npm)"),
    ("go.mod", "Go modules"),
    ("pyproject.toml", "Python (pyproject)"),
    ("setup.py", "Python (setuptools)"),
    ("requirements.txt", "Python (pip)"),
    ("pom.xml", "Java (Maven)"),
    ("build.gradle", "JVM (Gradle)"),
    ("build.gradle.kts", "JVM (Gradle Kotlin)"),
    ("Gemfile", "Ruby (Bundler)"),
    ("composer.json", "PHP (Composer)"),
    ("Makefile", "Make"),
    ("CMakeLists.txt", "CMake"),
    ("mix.exs", "Elixir (Mix)"),
    ("Package.swift", "Swift Package Manager"),
    ("deno.json", "Deno"),
];

/// The understanding report of a workspace.
pub fn render(ws: &Workspace, root: &str, s: Style) -> String {
    let mut out = String::new();
    let project = &ws.project;
    let rules = enabled_rules(&ws.packs, &ws.config, project, &[]);

    let _ = writeln!(
        out,
        "{}",
        s.bold("Reverse Codemosis: what it understands about this project")
    );
    let _ = writeln!(out, "  project   {root}");
    let protected = project.files.iter().filter(|f| !f.role.is_source()).count();
    let _ = writeln!(
        out,
        "  files     {} text files · {} protected (generated or vendored) · {} not read",
        project.files.len(),
        protected,
        project.skipped.len()
    );

    // Languages.
    let mut stats: BTreeMap<&'static str, LangStats> = BTreeMap::new();
    for f in &project.files {
        let name = f.language_name();
        let e = stats.entry(name).or_insert_with(|| LangStats {
            name,
            kind: f.linguist().map(|l| l.kind),
            color: f.linguist().and_then(|l| l.color),
            files: 0,
            bytes: 0,
            tier: Tier::Recognized,
            grammar: None,
            rules: 0,
            dialects: BTreeMap::new(),
        });
        e.files += 1;
        e.bytes += f.text.len();
        e.tier = e.tier.max(f.tier());
        if let Some(l) = f.lang {
            e.grammar = Some(l.id());
        } else if let Some(d) = f
            .linguist()
            .and_then(|l| codemosis_core::lang::for_linguist(l.name))
            .and_then(|g| codemosis_core::lang::unread_dialect(g, &f.path, &f.text))
        {
            *e.dialects.entry(d).or_default() += 1;
        }
    }
    for st in stats.values_mut() {
        st.rules = rules
            .iter()
            .filter(|r| {
                project
                    .files
                    .iter()
                    .any(|f| f.language_name() == st.name && r.applies_to(f))
            })
            .count();
    }
    let mut langs: Vec<&LangStats> = stats.values().collect();
    langs.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(b.name)));
    let counted: usize = langs
        .iter()
        .filter(|l| matches!(l.kind, Some(Kind::Programming | Kind::Markup)))
        .map(|l| l.bytes)
        .sum();

    let _ = writeln!(
        out,
        "\n{}",
        s.bold("Languages (by size; percentages count programming and markup, as GitHub does)")
    );
    if counted > 0 && s.color {
        let width = 56usize;
        let mut bar = String::new();
        for l in langs
            .iter()
            .filter(|l| matches!(l.kind, Some(Kind::Programming | Kind::Markup)))
        {
            let cells = ((l.bytes * width) as f64 / counted as f64).round() as usize;
            bar.push_str(&paint_rgb(s, l.color, &"█".repeat(cells)));
        }
        let _ = writeln!(out, "  {bar}");
    }
    for l in &langs {
        let pct = if matches!(l.kind, Some(Kind::Programming | Kind::Markup)) && counted > 0 {
            format!("{:>5.1}%", l.bytes as f64 * 100.0 / counted as f64)
        } else {
            "     -".to_string()
        };
        let rules_note = if l.rules > 0 {
            format!(" · {} rule{}", l.rules, if l.rules == 1 { "" } else { "s" })
        } else {
            String::new()
        };
        let mut grammar = l.grammar.map(|g| format!(" ({g})")).unwrap_or_default();
        for (dialect, n) in &l.dialects {
            grammar.push_str(&format!(
                "; {n} {dialect} file{} read lexically",
                if *n == 1 { "" } else { "s" }
            ));
        }
        let _ = writeln!(
            out,
            "  {} {:<22} {pct} {:>4} file{}  {}{}{}",
            paint_rgb(s, l.color, "●"),
            l.name,
            l.files,
            if l.files == 1 { " " } else { "s" },
            tier_label(l.tier),
            grammar,
            s.dim(&rules_note)
        );
    }

    // Build and test.
    let _ = writeln!(out, "\n{}", s.bold("Build and test"));
    let markers: Vec<&str> = ECOSYSTEM_MARKERS
        .iter()
        .filter(|(f, _)| ws.root.join(f).is_file())
        .map(|(_, what)| *what)
        .collect();
    if !markers.is_empty() {
        let _ = writeln!(out, "  ecosystems  {}", markers.join(" · "));
    }
    let (section, detected) = ws.oracle_section();
    let how = if ws.config_path.is_some() && !detected {
        "from codemosis.toml"
    } else if detected {
        "detected"
    } else {
        "none found"
    };
    let _ = writeln!(out, "  oracle      {how}");
    if let Some(b) = &section.build {
        let _ = writeln!(out, "    build     {b}");
    }
    if let Some(t) = &section.test {
        let _ = writeln!(out, "    test      {t}");
    }
    // Formatters, from configuration files only (nothing is run).
    let formatters = codemosis_engine::formatter::Formatters::new(&ws.root, &ws.config.format, false);
    let survey = formatters.survey(
        project
            .files
            .iter()
            .filter_map(|f| Some((f.path.as_path(), f.lang?.id()))),
    );
    if !survey.is_empty() && ws.config.format.mode != codemosis_engine::formatter::Mode::Off {
        let list: Vec<String> = survey
            .iter()
            .map(|(g, n)| {
                format!(
                    "{} ({}, {n} file{})",
                    g.kind,
                    g.evidence,
                    if *n == 1 { "" } else { "s" }
                )
            })
            .collect();
        let _ = writeln!(out, "  formatting  {}", list.join(" · "));
    }

    // Definitions in deep files.
    let mut defs: BTreeMap<&'static str, BTreeMap<String, usize>> = BTreeMap::new();
    for f in &project.files {
        let (Some(l), Some(root)) = (f.lang, f.root()) else {
            continue;
        };
        if !f.role.is_source() {
            continue;
        }
        let mut stack = vec![root];
        let mut cursor = root.walk();
        while let Some(n) = stack.pop() {
            if let Some(k) = l.definition_label(&n, &f.text) {
                *defs.entry(f.language_name()).or_default().entry(k).or_insert(0) += 1;
            }
            stack.extend(n.children(&mut cursor));
        }
    }
    if !defs.is_empty() {
        let _ = writeln!(out, "\n{}", s.bold("Definitions (deep languages)"));
        for (lang, kinds) in &defs {
            let parts: Vec<String> = kinds.iter().map(|(k, n)| format!("{n} {}", pretty_kind(k))).collect();
            let _ = writeln!(out, "  {:<14} {}", lang, parts.join(" · "));
        }
    }

    // Protected and uncertain files.
    let protected_files: Vec<_> = project.files.iter().filter(|f| !f.role.is_source()).collect();
    if !protected_files.is_empty() {
        let _ = writeln!(out, "\n{}", s.bold("Protected: never edited"));
        for f in protected_files.iter().take(12) {
            let why = match &f.role {
                linguist::Role::Generated(w) => format!("generated: {w}"),
                linguist::Role::Vendored(w) => format!("vendored: {w}"),
                linguist::Role::Source => unreachable!(),
            };
            let _ = writeln!(out, "  {:<40} {}", f.display_path(), s.dim(&why));
        }
        if protected_files.len() > 12 {
            let _ = writeln!(out, "  ... and {} more", protected_files.len() - 12);
        }
    }
    let guesses: Vec<_> = project
        .files
        .iter()
        .filter(|f| f.detection.language.is_some() && f.detection.confidence == Confidence::Guess)
        .collect();
    if !guesses.is_empty() {
        let _ = writeln!(out, "\n{}", s.bold("Uncertain language: universal rules only"));
        for f in guesses.iter().take(12) {
            let names: Vec<&str> = f.detection.candidates.iter().map(|c| c.name).collect();
            let _ = writeln!(
                out,
                "  {:<40} {}",
                f.display_path(),
                s.dim(&format!("could be {}", names.join(", ")))
            );
        }
    }

    // Distance.
    let scan = ws.scan();
    let distance = codemosis_core::Distance::from_findings(&scan.findings);
    let fixable = scan.findings.iter().filter(|f| f.is_fixable()).count();
    let questions = scan
        .findings
        .iter()
        .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
        .count();
    let _ = writeln!(out, "\n{}", s.bold("Distance from best form"));
    let _ = writeln!(out, "  {}", report::distance(&distance, s));
    let _ = writeln!(
        out,
        "  {fixable} provably fixable · {questions} need your decision · {} rules from {} packs apply",
        rules.len(),
        ws.packs.iter().filter(|p| p.applies_to(&project.languages())).count()
    );
    out
}

fn tier_of(l: &linguist::LinguistLanguage) -> Tier {
    if lang::for_linguist(l.name).is_some() {
        Tier::Deep
    } else if lex::profile_for(l).is_some() {
        Tier::Lexical
    } else {
        Tier::Recognized
    }
}

/// The whole catalogue with each language's tier.
pub fn languages(search: Option<&str>, tier: Option<&str>, s: Style) -> Result<String> {
    let wanted = match tier {
        None => None,
        Some("deep") => Some(Tier::Deep),
        Some("lexical") => Some(Tier::Lexical),
        Some("recognized") | Some("recognised") => Some(Tier::Recognized),
        Some(other) => bail!("--tier must be deep, lexical or recognized, not '{other}'"),
    };
    let mut out = String::new();
    let mut counts: BTreeMap<Tier, usize> = BTreeMap::new();
    let mut shown = 0;
    for l in linguist::all() {
        let t = tier_of(l);
        *counts.entry(t).or_insert(0) += 1;
        if wanted.is_some_and(|w| w != t) {
            continue;
        }
        if search.is_some_and(|q| !l.name.to_lowercase().contains(&q.to_lowercase())) {
            continue;
        }
        shown += 1;
        let exts: Vec<&str> = l.extensions.iter().take(4).copied().collect();
        let detail = match t {
            Tier::Deep => lang::for_linguist(l.name)
                .map(|g| format!("grammar {}", g.id()))
                .unwrap_or_default(),
            Tier::Lexical => lex::profile_for(l)
                .map(|p| format!("{} comments", p.family))
                .unwrap_or_default(),
            Tier::Recognized => String::new(),
        };
        let _ = writeln!(
            out,
            "  {} {:<32} {:<12} {:<11} {:<26} {}",
            paint_rgb(s, l.color, "●"),
            l.name,
            l.kind.as_str(),
            t.as_str(),
            exts.join(" "),
            s.dim(&detail)
        );
    }
    let total: usize = counts.values().sum();
    let _ = writeln!(
        out,
        "\n{} languages known to GitHub: {} deep · {} lexical · {} recognized{}",
        total,
        counts.get(&Tier::Deep).copied().unwrap_or(0),
        counts.get(&Tier::Lexical).copied().unwrap_or(0),
        counts.get(&Tier::Recognized).copied().unwrap_or(0),
        if shown < total {
            format!(" ({shown} shown)")
        } else {
            String::new()
        }
    );
    Ok(out)
}

/// One row of the language compatibility matrix.
fn matrix_row(l: &linguist::LinguistLanguage, packs: &[codemosis_core::Pack]) -> serde_json::Value {
    let t = tier_of(l);
    let grammar = lang::for_linguist(l.name);
    let mut v = serde_json::json!({
        "name": l.name,
        "kind": l.kind.as_str(),
        "tier": t.as_str(),
        "extensions": l.extensions,
        "filenames": l.filenames,
        "interpreters": l.interpreters,
    });
    // Text rules apply to every file whose language they name (or, for
    // universal rules, to every file they can read), deep files included.
    let text_rules: Vec<String> = packs
        .iter()
        .filter(|p| p.is_text())
        .flat_map(|p| p.rules.iter())
        .filter(|r| {
            let langs = r.pattern_languages();
            if langs.is_empty() {
                r.spec.within.as_deref() == Some("any") || t != Tier::Recognized
            } else {
                langs.iter().any(|x| x == l.name)
            }
        })
        .map(|r| r.id.clone())
        .collect();
    match grammar {
        Some(g) => {
            let mut rules: Vec<String> = packs
                .iter()
                .filter(|p| p.languages.iter().any(|x| x.id() == g.id()))
                .flat_map(|p| p.rules.iter().map(|r| r.id.clone()))
                .collect();
            rules.extend(text_rules);
            v["grammar"] = serde_json::json!({
                "id": g.id(),
                "crate": lang::grammar_crate(g.id()),
                "version": lang::grammar_version(g.id()),
                "facts": g.facts().iter().map(|f| format!("{}.{}", g.prefix(), f.name)).collect::<Vec<_>>(),
                "runner": g.runner().map(|r| r.describe()),
                "binding_analysis": g.id() == "python",
                "edit_guards": match g.id() {
                    "c" | "cpp" | "csharp" | "swift" => vec!["conditional compilation", "parse damage", "comments"],
                    "rust" => vec!["#[cfg]", "macro bodies", "parse damage", "comments"],
                    "bash" => vec!["parse damage", "comments", "zsh read lexically"],
                    _ => vec!["parse damage", "comments"],
                },
            });
            v["rules"] = serde_json::json!(rules);
        }
        None => {
            if let Some(p) = lex::profile_for(l) {
                v["lexical_family"] = serde_json::json!(p.family);
            }
            v["rules"] = serde_json::json!(text_rules);
        }
    }
    v
}

/// The language compatibility matrix as JSON: every language GitHub knows,
/// with its tier, grammar version, facts, runner, guards and rules.
pub fn languages_json() -> Result<String> {
    let packs = codemosis_core::pack::builtin().map_err(|e| anyhow::anyhow!("{e}"))?;
    let rows: Vec<serde_json::Value> = linguist::all().iter().map(|l| matrix_row(l, &packs)).collect();
    let v = serde_json::json!({
        "schema": "codemosis/languages/v1",
        "engine": codemosis_core::version::ENGINE_VERSION,
        "tree_sitter": lang::runtime_version(),
        "languages": rows,
    });
    Ok(serde_json::to_string_pretty(&v)? + "\n")
}

/// The matrix as Markdown, for docs/LANGUAGES.md.
pub fn languages_markdown() -> Result<String> {
    let packs = codemosis_core::pack::builtin().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut counts: BTreeMap<Tier, usize> = BTreeMap::new();
    for l in linguist::all() {
        *counts.entry(tier_of(l)).or_insert(0) += 1;
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# Languages\n\nGenerated by `codemosis languages --format markdown`; \
         `--format json` gives the same matrix for tools.\n\n\
         Every one of the {} languages in GitHub Linguist is known: **{}** deep (a syntax tree), \
         **{}** lexical (comments and strings), **{}** recognized (name, extensions, filenames, \
         interpreters). Built with tree-sitter {}.\n",
        counts.values().sum::<usize>(),
        counts.get(&Tier::Deep).copied().unwrap_or(0),
        counts.get(&Tier::Lexical).copied().unwrap_or(0),
        counts.get(&Tier::Recognized).copied().unwrap_or(0),
        lang::runtime_version().unwrap_or("?")
    );
    let _ = writeln!(s, "## Deep tier\n");
    let _ = writeln!(
        s,
        "| Language | Grammar | Version | Facts | Runner | Edit guards | Rules |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|---|---:|");
    for l in linguist::all().iter().filter(|l| tier_of(l) == Tier::Deep) {
        let r = matrix_row(l, &packs);
        let g = &r["grammar"];
        let facts = g["facts"].as_array().map(|a| a.len()).unwrap_or(0);
        let mut facts = format!("generic + {facts}");
        if g["binding_analysis"] == true {
            facts.push_str(" + bindings");
        }
        let guards: Vec<&str> = g["edit_guards"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|x| x.as_str())
            .collect();
        let _ = writeln!(
            s,
            "| {} | `{}` | {} {} | {} | {} | {} | {} |",
            l.name,
            g["id"].as_str().unwrap_or(""),
            g["crate"].as_str().unwrap_or(""),
            g["version"].as_str().unwrap_or("?"),
            facts,
            g["runner"]
                .as_str()
                .map(|x| format!("`{x}`"))
                .unwrap_or_else(|| "none".into()),
            guards.join(", "),
            r["rules"].as_array().map(|a| a.len()).unwrap_or(0)
        );
    }
    let mut families: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for l in linguist::all().iter().filter(|l| tier_of(l) == Tier::Lexical) {
        if let Some(p) = lex::profile_for(l) {
            families.entry(p.family.to_string()).or_default().push(l.name);
        }
    }
    let _ = writeln!(
        s,
        "\n## Lexical tier\n\nThe engine tells code from comments and strings, so text rules \
         (fixed waits, merge conflicts, committed keys) run only where a match is really code. \
         Grouped by comment syntax:\n"
    );
    for (family, names) in &families {
        let _ = writeln!(s, "- **{family}** ({}): {}", names.len(), names.join(", "));
    }
    let _ = writeln!(
        s,
        "\n## Recognized tier\n\nThe other {} languages are identified exactly (and never edited \
         on a guess); rules that read any text, such as unresolved merge conflicts and committed \
         private keys, apply to them. `codemosis languages --tier recognized` lists them.",
        counts.get(&Tier::Recognized).copied().unwrap_or(0)
    );
    Ok(s)
}
