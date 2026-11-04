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
        });
        e.files += 1;
        e.bytes += f.text.len();
        e.tier = e.tier.max(f.tier());
        if let Some(l) = f.lang {
            e.grammar = Some(l.id());
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
        let grammar = l.grammar.map(|g| format!(" ({g})")).unwrap_or_default();
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

    // Definitions in deep files.
    let mut defs: BTreeMap<&'static str, BTreeMap<&'static str, usize>> = BTreeMap::new();
    for f in &project.files {
        let (Some(l), Some(root)) = (f.lang, f.root()) else {
            continue;
        };
        if !f.role.is_source() {
            continue;
        }
        let kinds = l.definition_kinds();
        let mut stack = vec![root];
        let mut cursor = root.walk();
        while let Some(n) = stack.pop() {
            if let Some((k, _)) = kinds.iter().find(|(k, _)| *k == n.kind()) {
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
