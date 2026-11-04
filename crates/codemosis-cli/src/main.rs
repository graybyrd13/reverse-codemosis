//! `codemosis`: correct code the way a proof corrects an argument.

mod scaffold;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use codemosis_core::selftest::{self, Outcome};
use codemosis_core::{lang, pack, Category, Finding, Pack, Status};
use codemosis_engine::answers::Answer;
use codemosis_engine::report::{self, Style};
use codemosis_engine::session::{Event, Options, Session};
use codemosis_engine::{config, enabled_rules, oracle_from, vcs, Workspace};
use std::io::{IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "codemosis",
    version,
    about = "Reverse Codemosis: correct code the way a proof corrects an argument.",
    long_about = "Reverse Codemosis finds every place code falls short of its best form, applies \
                  the corrections it can prove, verifies each step against your build and tests, \
                  explains what it cannot fix, and asks when a decision is yours. It never guesses."
)]
struct Cli {
    /// When to use colour.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true)]
    color: ColorChoice,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum Format {
    Text,
    Json,
    Markdown,
}

#[derive(Subcommand)]
enum Command {
    /// Measure a project against its packs and report every finding. Changes nothing.
    Check {
        /// Project root.
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Also load the pack in this directory.
        #[arg(long = "pack", value_name = "DIR")]
        packs: Vec<PathBuf>,
        /// Only run these rules (or packs).
        #[arg(long = "rule", value_name = "ID")]
        rules: Vec<String>,
    },
    /// Apply every correction that can be proven, verifying each step with the build and tests.
    Fix {
        /// Project root.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Plan the corrections in memory and show them; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Run even if the git working tree has uncommitted changes.
        #[arg(long)]
        allow_dirty: bool,
        /// Run even if the project is not under git.
        #[arg(long)]
        allow_no_vcs: bool,
        /// Do not run the build and tests (structural checks and rule proofs only).
        #[arg(long)]
        no_verify: bool,
        /// Print the full diff at the end.
        #[arg(long)]
        diff: bool,
        /// Write a report: Markdown, or JSON if the name ends in .json.
        #[arg(long, value_name = "FILE")]
        report: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        #[arg(long = "pack", value_name = "DIR")]
        packs: Vec<PathBuf>,
        #[arg(long = "rule", value_name = "ID")]
        rules: Vec<String>,
        /// Stop after this many attempted steps.
        #[arg(long, default_value_t = 500)]
        max_steps: usize,
    },
    /// Record your answer to a question. "yes" applies the correction; "no" keeps the code.
    Answer {
        /// The question id, like Q-1a2b3c4d.
        id: String,
        #[arg(value_parser = ["yes", "no"])]
        answer: String,
        /// Project root.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// A note recorded with the answer.
        #[arg(long)]
        note: Option<String>,
    },
    /// Explain a rule: what it finds, why, the facts it requires and its proof.
    Explain {
        /// Rule id, like python-core/is-literal.
        rule: String,
        #[arg(long = "pack", value_name = "DIR")]
        packs: Vec<PathBuf>,
    },
    /// List the available rules.
    Rules {
        #[arg(long = "pack", value_name = "DIR")]
        packs: Vec<PathBuf>,
    },
    /// Work with fix packs.
    Pack {
        #[command(subcommand)]
        command: PackCommand,
    },
    /// Print the syntax tree of a file, to help write rule shapes.
    Tree { file: PathBuf },
    /// List the fact vocabulary, template filters and edit operations rules can use.
    Facts {
        /// Only this language's facts.
        #[arg(long)]
        language: Option<String>,
    },
    /// Write a codemosis.toml for a project, with the build and test commands detected.
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum PackCommand {
    /// Run the self-tests of packs: every example and counterexample of every rule.
    Test {
        /// Pack directories (default: the built-in packs).
        dirs: Vec<PathBuf>,
        /// Do not execute examples; check rewrites only.
        #[arg(long)]
        no_run: bool,
        /// Only rules whose id contains this text.
        #[arg(long)]
        rule: Option<String>,
        /// Show passing checks too.
        #[arg(long, short)]
        verbose: bool,
    },
    /// Create a new pack with one working rule to start from.
    New {
        dir: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long, default_value = "python")]
        language: String,
    },
}

fn style(choice: ColorChoice, stream_is_tty: bool) -> Style {
    let color = match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => stream_is_tty && std::env::var_os("NO_COLOR").is_none(),
    };
    Style { color }
}

fn main() -> ExitCode {
    // Behave like other command-line tools when output is piped into
    // something that stops reading early (`codemosis rules | head`).
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let out = style(cli.color, std::io::stdout().is_terminal());
    let err = style(cli.color, std::io::stderr().is_terminal());
    match run(cli.command, out, err) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {e:#}", err.red("error:"));
            ExitCode::from(2)
        }
    }
}

/// The packs written for a language the project contains.
fn applicable<'p>(packs: &'p [Pack], languages: &[&str]) -> Vec<&'p Pack> {
    packs.iter().filter(|p| languages.contains(&p.language.id())).collect()
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("cannot open {}", path.display()))
}

fn all_packs(extra: &[PathBuf]) -> Result<Vec<Pack>> {
    let mut packs = pack::builtin().map_err(|e| anyhow!("{e}"))?;
    for dir in extra {
        let p = pack::compile(&pack::read_dir(dir).map_err(|e| anyhow!("{e}"))?).map_err(|e| anyhow!("{e}"))?;
        packs.retain(|q| q.meta.id != p.meta.id);
        packs.push(p);
    }
    Ok(packs)
}

fn display_root(root: &Path) -> String {
    let cwd = std::env::current_dir().ok();
    match cwd.as_deref().and_then(|c| root.strip_prefix(c).ok()) {
        Some(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Some(rel) => rel.display().to_string(),
        None => root.display().to_string(),
    }
}

fn run(command: Command, s: Style, e: Style) -> Result<ExitCode> {
    match command {
        Command::Check {
            path,
            format,
            packs,
            rules,
        } => check(&path, format, &packs, &rules, s),
        Command::Fix {
            path,
            dry_run,
            allow_dirty,
            allow_no_vcs,
            no_verify,
            diff,
            report,
            format,
            packs,
            rules,
            max_steps,
        } => {
            let root = canonical(&path)?;
            if !dry_run {
                match vcs::state(&root) {
                    vcs::TreeState::Clean => {}
                    vcs::TreeState::Dirty(paths) if !allow_dirty => bail!(
                        "the working tree has uncommitted changes ({}{}).\n\
                         Commit or stash them first, so `git diff` shows exactly what Reverse Codemosis changed, or pass --allow-dirty.",
                        paths.iter().take(5).cloned().collect::<Vec<_>>().join(", "),
                        if paths.len() > 5 { ", ..." } else { "" }
                    ),
                    vcs::TreeState::NotARepository | vcs::TreeState::Unknown if !allow_no_vcs => bail!(
                        "{} is not in a git repository, so the corrections could not be reviewed or undone with git.\n\
                         Put it under git first, or pass --allow-no-vcs.",
                        root.display()
                    ),
                    _ => {}
                }
            }
            fix(
                &root,
                dry_run,
                no_verify,
                diff,
                report.as_deref(),
                format,
                &packs,
                &rules,
                max_steps,
                s,
                e,
            )
        }
        Command::Answer { id, answer, path, note } => answer_cmd(&id, &answer, &path, note, s),
        Command::Explain { rule, packs } => explain(&rule, &packs, s),
        Command::Rules { packs } => list_rules(&packs, s),
        Command::Pack { command } => match command {
            PackCommand::Test {
                dirs,
                no_run,
                rule,
                verbose,
            } => pack_test(&dirs, !no_run, rule.as_deref(), verbose, s),
            PackCommand::New { dir, id, language } => {
                scaffold::new_pack(&dir, &id, &language)?;
                println!("Created pack '{id}' in {}.", dir.display());
                println!(
                    "Run `codemosis pack test {}` to check it, then add your own rules.",
                    dir.display()
                );
                Ok(ExitCode::SUCCESS)
            }
        },
        Command::Tree { file } => {
            let lang =
                lang::for_path(&file).ok_or_else(|| anyhow!("no language is registered for {}", file.display()))?;
            let text = std::fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))?;
            let f = codemosis_core::SourceFile::parse(&file, lang, text)?;
            print!("{}", codemosis_core::render_tree(&f));
            Ok(ExitCode::SUCCESS)
        }
        Command::Facts { language } => facts(language.as_deref(), s),
        Command::Init { path } => init(&path),
    }
}

fn check(path: &Path, format: Format, extra: &[PathBuf], only: &[String], s: Style) -> Result<ExitCode> {
    let root = canonical(path)?;
    let ws = Workspace::open(&root, extra)?;
    let rules = enabled_rules(&ws.packs, &ws.config, &ws.project, only);
    let answers = |id: &str| ws.answers.get(id);
    let scan = codemosis_core::scan(&ws.project, rules, &answers, None);
    let open: Vec<&Finding> = scan
        .findings
        .iter()
        .filter(|f| !matches!(f.status, Status::Declined { .. }))
        .collect();
    let distance = codemosis_core::Distance::from_findings(&scan.findings);
    match format {
        Format::Json => {
            let v = serde_json::json!({
                "project": root.display().to_string(),
                "distance": distance,
                "findings": scan.findings,
                "rule_errors": scan.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Format::Markdown => {
            println!("# Reverse Codemosis findings\n\nDistance: {distance}\n");
            for f in &open {
                println!(
                    "- **{}** `{}:{}` ({}): {}",
                    f.rule,
                    f.file.display(),
                    f.span.start.line,
                    f.status.label(),
                    f.message
                );
            }
        }
        Format::Text => {
            println!(
                "{}",
                s.bold(&format!("Reverse Codemosis {}", env!("CARGO_PKG_VERSION")))
            );
            println!("  project   {}", display_root(&root));
            println!(
                "  packs     {}\n",
                report::packs_line(&applicable(&ws.packs, &ws.project.languages()))
            );
            for c in Category::ALL {
                for f in open.iter().filter(|f| f.category == c) {
                    println!("{}", report::finding(f, &ws.project, s));
                }
            }
            for err in &scan.errors {
                println!("{} {err}", s.red("rule error:"));
            }
            let fixable = open.iter().filter(|f| f.is_fixable()).count();
            let questions = open
                .iter()
                .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
                .count();
            let other = open.len() - fixable - questions;
            println!("{}", s.bold("Summary"));
            println!("  distance  {}", report::distance(&distance, s));
            println!(
                "  findings  {} provably fixable · {} need your decision · {} explained",
                fixable, questions, other
            );
            if fixable > 0 {
                println!("\nRun `codemosis fix` to apply the provable corrections, verified by your build and tests.");
            }
        }
    }
    Ok(if open.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

#[allow(clippy::too_many_arguments)]
fn fix(
    root: &Path,
    dry_run: bool,
    no_verify: bool,
    show_diff: bool,
    report_path: Option<&Path>,
    format: Format,
    extra: &[PathBuf],
    only: &[String],
    max_steps: usize,
    s: Style,
    e: Style,
) -> Result<ExitCode> {
    let mut ws = Workspace::open(root, extra)?;
    let (section, detected) = ws.oracle_section();
    let oracle = (!no_verify && !dry_run).then(|| oracle_from(root, &section));
    let text = format == Format::Text;
    let languages = ws.project.languages();
    let shown = applicable(&ws.packs, &languages);
    if text {
        print!(
            "{}",
            report::header(
                &report::Header {
                    root: &display_root(root),
                    packs: &shown,
                    build: oracle.as_ref().and(section.build.as_deref()),
                    test: oracle.as_ref().and(section.test.as_deref()),
                    detected,
                },
                s
            )
        );
        let _ = std::io::stdout().flush();
    }
    let rules = enabled_rules(&ws.packs, &ws.config, &ws.project, only);
    let answers_ledger = ws.answers.clone();
    let answers = |id: &str| answers_ledger.get(id);
    let session = Session {
        project: &mut ws.project,
        rules,
        answers: &answers,
        oracle: oracle.as_ref(),
        options: Options {
            write: !dry_run,
            max_steps,
        },
    };
    let live = std::io::stderr().is_terminal() && text;
    if live && oracle.as_ref().is_some_and(|o| o.is_configured()) {
        eprintln!("{}", e.dim("  measuring: running the build and tests..."));
    }
    let report = session.run(&mut |ev| {
        if !live {
            return;
        }
        match ev {
            Event::Baseline(_) => eprintln!("{}", e.dim("  baseline measured; correcting...")),
            Event::Step(st) => eprintln!(
                "{}",
                e.dim(&format!(
                    "  ✓ {} {} ({} change(s))",
                    st.number,
                    st.rule,
                    st.findings.len()
                ))
            ),
            Event::Rejected(r) => eprintln!("{}", e.dim(&format!("  ✗ {} rejected", r.finding.rule))),
        }
    })?;
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        Format::Markdown => println!("{}", report::session_markdown(&report, &display_root(root), &shown)),
        Format::Text => print!(
            "{}",
            report::session_text(&report, &ws.project, s, show_diff || dry_run)
        ),
    }
    if let Some(p) = report_path {
        let body = if p.extension().is_some_and(|x| x == "json") {
            serde_json::to_string_pretty(&report)?
        } else {
            report::session_markdown(&report, &display_root(root), &shown)
        };
        std::fs::write(p, body).with_context(|| format!("cannot write {}", p.display()))?;
        if text {
            println!("Report written to {}", p.display());
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn answer_cmd(id: &str, answer: &str, path: &Path, note: Option<String>, s: Style) -> Result<ExitCode> {
    let root = canonical(path)?;
    let mut ws = Workspace::open(&root, &[])?;
    let scan = ws.scan();
    let finding = scan
        .findings
        .iter()
        .find(|f| f.question().is_some_and(|q| q.id.eq_ignore_ascii_case(id)))
        .ok_or_else(|| {
            anyhow!(
                "there is no question {id} in {}; run `codemosis check` to see the open questions",
                root.display()
            )
        })?;
    let q = finding.question().expect("found by question");
    ws.answers.set(Answer {
        id: q.id.clone(),
        rule: finding.rule.clone(),
        location: format!("{}:{}", finding.file.display(), finding.span.start.line),
        question: q.text.clone(),
        answer: answer.to_string(),
        note,
    });
    ws.answers.save(&root).map_err(|e| anyhow!(e))?;
    println!(
        "Recorded {} to {} in {}.",
        s.bold(answer),
        q.id,
        codemosis_engine::answers::FILE
    );
    if answer == "yes" {
        println!("Run `codemosis fix` to apply the correction.");
    } else {
        println!("The code stays as it is and will not be reported again.");
    }
    Ok(ExitCode::SUCCESS)
}

fn find_rule<'p>(packs: &'p [Pack], id: &str) -> Option<&'p codemosis_core::CompiledRule> {
    packs
        .iter()
        .flat_map(|p| p.rules.iter())
        .find(|r| r.id == id || r.spec.id == id)
}

fn explain(id: &str, extra: &[PathBuf], s: Style) -> Result<ExitCode> {
    let packs = all_packs(extra)?;
    let rule = find_rule(&packs, id).ok_or_else(|| anyhow!("no rule named '{id}'; `codemosis rules` lists them"))?;
    let spec = &rule.spec;
    println!("{}  {}", s.bold(&rule.id), spec.title);
    let kind = match (&spec.fix, &spec.ask) {
        (Some(_), None) => format!(
            "corrects automatically ({})",
            spec.semantics.map(|x| x.describe()).unwrap_or("")
        ),
        (Some(_), Some(_)) => format!(
            "corrects after you answer yes ({})",
            spec.semantics.map(|x| x.describe()).unwrap_or("")
        ),
        _ => "explains only".to_string(),
    };
    println!("  category  {} — {}", spec.category, spec.category.describe());
    println!("  language  {}", rule.language.id());
    println!("  action    {kind}\n");
    println!("{}\n", spec.explain.trim());
    println!("{}", s.bold("Shape"));
    for l in spec.shape.trim().lines() {
        println!("  {l}");
    }
    if !spec.facts.is_empty() {
        println!("\n{}", s.bold("Facts that must hold"));
        for f in &spec.facts {
            println!("  {f}");
        }
    }
    if let Some(ask) = &spec.ask {
        println!("\n{}\n  {}", s.bold("Question"), ask.question);
    }
    if let Some(ex) = spec.examples.first() {
        println!("\n{}", s.bold("Example"));
        match &ex.after {
            Some(after) => print!(
                "{}",
                codemosis_engine::session::unified_diff(rule.language.example_file(), &ex.before, after)
            ),
            None => {
                for l in ex.before.lines() {
                    println!("  {l}");
                }
            }
        }
    }
    println!(
        "\n{} example(s) and {} counterexample(s) prove this rule; run `codemosis pack test --rule {}`.",
        spec.examples.len(),
        spec.counterexamples.len(),
        spec.id
    );
    for r in &spec.references {
        println!("See also: {r}");
    }
    Ok(ExitCode::SUCCESS)
}

fn list_rules(extra: &[PathBuf], s: Style) -> Result<ExitCode> {
    for p in all_packs(extra)? {
        println!(
            "{} {}  {}  [{}]",
            s.bold(&p.meta.id),
            p.meta.version,
            p.meta.summary,
            p.language.id()
        );
        for r in &p.rules {
            let kind = match (&r.spec.fix, &r.spec.ask) {
                (Some(_), None) => "fix",
                (Some(_), Some(_)) => "ask",
                _ => "explain",
            };
            println!(
                "  {} {:<7} {:<44} {}",
                s.category(r.spec.category),
                kind,
                r.id,
                s.dim(&r.spec.title)
            );
        }
        println!();
    }
    Ok(ExitCode::SUCCESS)
}

fn pack_test(dirs: &[PathBuf], run: bool, filter: Option<&str>, verbose: bool, s: Style) -> Result<ExitCode> {
    let packs: Vec<Pack> = if dirs.is_empty() {
        pack::builtin().map_err(|e| anyhow!("{e}"))?
    } else {
        let mut v = Vec::new();
        for d in dirs {
            let src = pack::read_dir(d).map_err(|e| anyhow!("{e}"))?;
            match pack::compile(&src) {
                Ok(p) => v.push(p),
                Err(e) => {
                    println!("{} {e}", s.red("✗"));
                    return Ok(ExitCode::from(1));
                }
            }
        }
        v
    };
    let mut totals = (0, 0, 0);
    for p in &packs {
        let mut r = selftest::test_pack(p, &selftest::Options { run });
        if let Some(f) = filter {
            r.rules.retain(|rr| rr.rule.contains(f));
        }
        println!("{} {} ({})", s.bold(&p.meta.id), p.meta.version, p.origin);
        for rr in &r.rules {
            let mark = if rr.passed() { s.green("✓") } else { s.red("✗") };
            let checks = rr.checks.len();
            println!("  {mark} {} {}", rr.rule, s.dim(&format!("({checks} checks)")));
            for c in &rr.checks {
                match &c.outcome {
                    Outcome::Pass if verbose => println!("      {} {}", s.green("ok"), c.name),
                    Outcome::Pass => {}
                    Outcome::Skip(why) => println!("      {} {} ({why})", s.yellow("skip"), c.name),
                    Outcome::Fail(why) => {
                        println!("      {} {}", s.red("FAIL"), c.name);
                        for l in why.lines() {
                            println!("           {l}");
                        }
                    }
                }
            }
        }
        let (a, b, c) = r.counts();
        totals = (totals.0 + a, totals.1 + b, totals.2 + c);
    }
    println!(
        "\n{} passed · {} failed · {} skipped",
        totals.0,
        if totals.1 > 0 {
            s.red(&totals.1.to_string())
        } else {
            totals.1.to_string()
        },
        totals.2
    );
    Ok(if totals.1 == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn facts(language: Option<&str>, s: Style) -> Result<ExitCode> {
    if language.is_none() {
        println!("{}", s.bold("Facts every language can use"));
        for f in codemosis_core::eval::GENERIC_FACTS {
            println!("  {:<46} {}", f.signature, s.dim(f.doc));
        }
        println!("\n{}", s.bold("Template filters"));
        for (n, d) in codemosis_core::template::GENERIC_FILTERS {
            println!("  {:<22} {}", n, s.dim(d));
        }
        println!("\n{}", s.bold("Edit operations"));
        for (n, d) in [
            ("replace", "replace a capture with `with`"),
            ("delete", "delete a capture's text"),
            ("delete_lines", "delete the whole lines a capture occupies"),
            ("delete_item", "delete a list item with its comma"),
            ("insert_before", "insert `with` before a capture"),
            ("insert_after", "insert `with` after a capture"),
            (
                "insert_lines_before",
                "insert lines before a capture's line, at its indentation",
            ),
            (
                "insert_lines_after",
                "insert lines after a capture's line, at its indentation",
            ),
            ("add_import", "add an import statement where the language puts imports"),
        ] {
            println!("  {:<22} {}", n, s.dim(d));
        }
    }
    for l in lang::all() {
        if language.is_some_and(|x| x != l.id()) {
            continue;
        }
        if l.facts().is_empty() && l.filters().is_empty() {
            continue;
        }
        println!("\n{}", s.bold(&format!("{} facts ({}.*)", l.id(), l.prefix())));
        for f in l.facts() {
            println!("  {:<46} {}", f.signature, s.dim(f.doc));
        }
        for (n, d) in l.filters() {
            println!("  filter {:<39} {}", format!("{}.{n}", l.prefix()), s.dim(d));
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn init(path: &Path) -> Result<ExitCode> {
    let root = canonical(path)?;
    let file = root.join(config::FILE_NAME);
    if file.exists() {
        bail!("{} already exists", file.display());
    }
    let o = config::detect_oracle(&root);
    let line = |key: &str, v: &Option<String>| match v {
        Some(cmd) => format!("{key} = {}\n", toml_string(cmd)),
        None => format!("# {key} = \"...\"\n"),
    };
    let mut text = String::from(
        "# Reverse Codemosis configuration.\n\
         # The oracle is your project's own build and test commands. Every correction\n\
         # is kept only if the build does not regress and no passing test fails.\n\n[oracle]\n",
    );
    text.push_str(&line("build", &o.build));
    text.push_str(&line("test", &o.test));
    if !o.env.is_empty() {
        let pairs: Vec<String> = o.env.iter().map(|(k, v)| format!("{k} = {}", toml_string(v))).collect();
        text.push_str(&format!("env = {{ {} }}\n", pairs.join(", ")));
    }
    text.push_str("# timeout = 600\n\n[rules]\n# disable = [\"python-core/redundant-pass\"]\n");
    std::fs::write(&file, text).with_context(|| format!("cannot write {}", file.display()))?;
    println!(
        "Wrote {}. Check the build and test commands, then run `codemosis check`.",
        file.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}
