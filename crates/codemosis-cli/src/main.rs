//! `codemosis`: correct code the way a proof corrects an argument.

mod interactive;
mod map;
mod packdocs;
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

#[derive(Clone, Copy, ValueEnum, PartialEq, Eq)]
enum CheckFormat {
    /// Compiler-style findings for a terminal.
    Text,
    /// Every finding, with fingerprints (schema codemosis/check/v1).
    Json,
    /// A Markdown summary, for a pull-request comment.
    Markdown,
    /// SARIF 2.1.0, for GitHub code scanning and other dashboards.
    Sarif,
    /// A GitLab Code Quality report.
    Gitlab,
    /// GitHub Actions annotations, plus a job summary in $GITHUB_STEP_SUMMARY.
    Github,
    /// A GitHub pull-request review with suggested changes, as JSON for the API.
    Review,
}

#[derive(Subcommand)]
enum Command {
    /// Measure a project against its packs and report every finding. Changes nothing.
    Check {
        /// Project root.
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, value_enum, default_value_t = CheckFormat::Text)]
        format: CheckFormat,
        /// Write the report to this file instead of standard output.
        #[arg(long, short, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Also load the pack in this directory.
        #[arg(long = "pack", value_name = "DIR")]
        packs: Vec<PathBuf>,
        /// Only run these rules (or packs).
        #[arg(long = "rule", value_name = "ID")]
        rules: Vec<String>,
        /// Exit 1 when a new open finding reaches this threshold: error, warning,
        /// note, any or never, or a category (findings in it or more serious ones).
        /// Default: [policy] fail_on, or any.
        #[arg(long, value_name = "THRESHOLD")]
        fail_on: Option<String>,
        /// Accept the findings recorded in this baseline; report and fail only on new ones.
        /// Default: [policy] baseline, if set.
        #[arg(long, value_name = "FILE")]
        baseline: Option<PathBuf>,
        /// Ignore any configured baseline.
        #[arg(long, conflicts_with = "baseline")]
        no_baseline: bool,
        /// Also list suppressed findings, with who suppressed them and why.
        #[arg(long)]
        show_suppressed: bool,
        /// Show the proven corrections as a diff (not verified by the build and tests; `fix` does that).
        #[arg(long)]
        diff: bool,
    },
    /// Record today's open findings as accepted, so `check --baseline` fails only on new ones.
    Baseline {
        /// Project root.
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Where to write it, relative to the project root.
        #[arg(long, short, default_value = codemosis_engine::policy::DEFAULT_BASELINE)]
        output: PathBuf,
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
        /// Ask every open question first, then correct with your answers.
        #[arg(long, short)]
        interactive: bool,
        /// Do not write a proof certificate to .codemosis/proofs.
        #[arg(long)]
        no_proof: bool,
    },
    /// Ask the open questions one by one and record your answers.
    Ask {
        /// Project root.
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long = "pack", value_name = "DIR")]
        packs: Vec<PathBuf>,
        #[arg(long = "rule", value_name = "ID")]
        rules: Vec<String>,
    },
    /// Record your answer to one question, such as "yes", "no", "keep" or "remove".
    Answer {
        /// The question id, like Q-1a2b3c4d.
        id: String,
        /// One of the question's answers.
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
    /// Show what Reverse Codemosis understands about a project: its languages,
    /// how deeply each is understood, its build and tests, its definitions.
    Map {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// List every language GitHub knows, and how deeply each is understood.
    Languages {
        /// Only languages whose name contains this text.
        #[arg(long)]
        search: Option<String>,
        /// Only languages understood at this depth: deep, lexical or recognized.
        #[arg(long)]
        tier: Option<String>,
    },
    /// Print the syntax tree of a file, to help write rule shapes.
    Tree { file: PathBuf },
    /// List the fact vocabulary, template filters and edit operations rules can use.
    Facts {
        /// Only this language's facts.
        #[arg(long)]
        language: Option<String>,
    },
    /// Work with proof certificates.
    Proof {
        #[command(subcommand)]
        command: ProofCommand,
    },
    /// Pin the packs a project uses by content hash in codemosis.lock.
    Lock {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Only check that the lock matches; change nothing.
        #[arg(long)]
        check: bool,
    },
    /// Write a codemosis.toml for a project, with the build and test commands detected.
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Subcommand)]
enum ProofCommand {
    /// Re-derive a recorded change: recover the original code from the patch and
    /// replay every step with the pinned rules, checking each reproduces exactly.
    Verify {
        /// The certificate (a .json file in .codemosis/proofs).
        certificate: PathBuf,
        /// Project root.
        #[arg(long, default_value = ".")]
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
        /// Mutation testing: remove each rule's facts one at a time and report the
        /// facts no example or counterexample needs.
        #[arg(long)]
        mutations: bool,
    },
    /// Find weaknesses in packs that still load: unused captures and tables,
    /// unproven semantics, thin explanations, missing keep options.
    Lint {
        /// Pack directories (default: the built-in packs).
        dirs: Vec<PathBuf>,
        /// Exit 1 on warnings too.
        #[arg(long)]
        deny_warnings: bool,
        /// Show notes as well as warnings.
        #[arg(long, short)]
        verbose: bool,
    },
    /// Write a Markdown reference page for each pack, generated from its rules.
    Docs {
        /// Pack directories (default: the built-in packs).
        dirs: Vec<PathBuf>,
        /// Output directory.
        #[arg(long, short, default_value = "docs/packs")]
        output: PathBuf,
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

/// The packs that apply to a project with these grammars.
fn applicable<'p>(packs: &'p [Pack], languages: &[&str]) -> Vec<&'p Pack> {
    packs.iter().filter(|p| p.applies_to(languages)).collect()
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
            output,
            packs,
            rules,
            fail_on,
            baseline,
            no_baseline,
            show_suppressed,
            diff,
        } => check(
            &path,
            CheckOptions {
                format,
                output: output.as_deref(),
                fail_on: fail_on.as_deref(),
                baseline: baseline.as_deref(),
                no_baseline,
                show_suppressed,
                diff,
            },
            &packs,
            &rules,
            s,
        ),
        Command::Baseline { path, output } => baseline_cmd(&path, &output),
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
            interactive,
            no_proof,
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
                FixOptions {
                    dry_run,
                    no_verify,
                    show_diff: diff,
                    report_path: report.as_deref(),
                    format,
                    max_steps,
                    interactive,
                    proof: !no_proof,
                },
                &packs,
                &rules,
                s,
                e,
            )
        }
        Command::Ask { path, packs, rules } => {
            let root = canonical(&path)?;
            let mut ws = Workspace::open(&root, &packs)?;
            let stdin = std::io::stdin();
            let recorded = interactive::ask(&mut ws, &rules, s, &mut stdin.lock(), &mut std::io::stdout())?;
            if recorded > 0 {
                println!("\nRun `codemosis fix` to apply the corrections you chose.");
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Map { path } => {
            let root = canonical(&path)?;
            let ws = Workspace::open(&root, &[])?;
            print!("{}", map::render(&ws, &display_root(&root), s));
            Ok(ExitCode::SUCCESS)
        }
        Command::Languages { search, tier } => {
            print!("{}", map::languages(search.as_deref(), tier.as_deref(), s)?);
            Ok(ExitCode::SUCCESS)
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
                mutations,
            } => {
                if mutations {
                    pack_mutations(&dirs, rule.as_deref(), verbose, s)
                } else {
                    pack_test(&dirs, !no_run, rule.as_deref(), verbose, s)
                }
            }
            PackCommand::Lint {
                dirs,
                deny_warnings,
                verbose,
            } => pack_lint(&dirs, deny_warnings, verbose, s),
            PackCommand::Docs { dirs, output } => {
                let packs = packs_from(&dirs)?;
                std::fs::create_dir_all(&output).with_context(|| format!("cannot create {}", output.display()))?;
                for p in &packs {
                    let file = output.join(format!("{}.md", p.meta.id));
                    std::fs::write(&file, packdocs::pack_page(p))
                        .with_context(|| format!("cannot write {}", file.display()))?;
                }
                std::fs::write(output.join("README.md"), packdocs::index_page(&packs))?;
                println!("Wrote {} pack page(s) to {}.", packs.len(), output.display());
                Ok(ExitCode::SUCCESS)
            }
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
            let text = std::fs::read_to_string(&file).with_context(|| format!("cannot read {}", file.display()))?;
            let f = codemosis_core::SourceFile::new(&file, text);
            if f.tree.is_none() {
                bail!(
                    "{} is {} ({} tier): this build has no grammar for it, so it has no syntax tree",
                    file.display(),
                    f.language_name(),
                    f.tier().as_str()
                );
            }
            print!("{}", codemosis_core::render_tree(&f));
            Ok(ExitCode::SUCCESS)
        }
        Command::Facts { language } => facts(language.as_deref(), s),
        Command::Init { path } => init(&path),
        Command::Lock { path, check } => lock_cmd(&path, check),
        Command::Proof {
            command: ProofCommand::Verify { certificate, path },
        } => proof_verify(&certificate, &path, s),
    }
}

struct CheckOptions<'a> {
    format: CheckFormat,
    output: Option<&'a Path>,
    fail_on: Option<&'a str>,
    baseline: Option<&'a Path>,
    no_baseline: bool,
    show_suppressed: bool,
    diff: bool,
}

fn finding_json(f: &Finding, policy: &codemosis_engine::policy::Policy, baselined: bool) -> serde_json::Value {
    let mut v = serde_json::to_value(f).unwrap_or_default();
    v["fingerprint"] = serde_json::json!(f.fingerprint());
    v["level"] = serde_json::json!(policy.level(&f.rule, f.category).as_str());
    v["confidence"] = serde_json::json!(f.confidence().as_str());
    if baselined {
        v["baselined"] = serde_json::json!(true);
    }
    v
}

/// The proven corrections of a scan applied in memory, as a diff: one
/// non-overlapping set of edits per file. Nothing is verified or written.
fn proposed_diff(project: &codemosis_core::Project, findings: &[&Finding]) -> String {
    let mut out = String::new();
    for file in &project.files {
        let mut chosen: Vec<codemosis_core::Edit> = Vec::new();
        for f in findings.iter().filter(|f| f.file == file.path && f.is_fixable()) {
            let clash = f
                .edits
                .iter()
                .any(|a| chosen.iter().any(|b| codemosis_core::edit::overlaps(a, b)));
            if !clash {
                chosen.extend(f.edits.iter().cloned());
            }
        }
        if chosen.is_empty() {
            continue;
        }
        if let Ok(new) = codemosis_core::edit::apply(&file.text, &chosen) {
            out.push_str(&codemosis_engine::session::unified_diff(
                &file.display_path(),
                &file.text,
                &new,
            ));
        }
    }
    codemosis_core::redact::redact(&out).into_owned()
}

fn check(path: &Path, o: CheckOptions, extra: &[PathBuf], only: &[String], s: Style) -> Result<ExitCode> {
    use codemosis_engine::ci;
    use codemosis_engine::policy::{Baseline, FailOn};
    let root = canonical(path)?;
    let ws = Workspace::open(&root, extra)?;
    let rules = enabled_rules(&ws.packs, &ws.config, &ws.project, only);
    let scan = ws.scan_rules(only);
    let fail_on = match o.fail_on {
        Some(t) => FailOn::parse(t).map_err(|e| anyhow!("--fail-on: {e}"))?,
        None => ws.policy.fail_on,
    };
    let baseline_path = match (o.no_baseline, o.baseline, &ws.config.policy.baseline) {
        (true, _, _) => None,
        (false, Some(p), _) => Some(p.to_path_buf()),
        (false, None, Some(p)) => Some(root.join(p)),
        _ => None,
    };
    let baseline = match &baseline_path {
        Some(p) => Some(Baseline::load(p).map_err(|e| anyhow!(e))?),
        None => None,
    };
    let baselined: Option<std::collections::BTreeSet<String>> = baseline.as_ref().map(|b| {
        scan.findings
            .iter()
            .filter(|f| b.contains(f))
            .map(|f| f.fingerprint())
            .collect()
    });
    let out = ci::CheckOutput {
        project: &ws.project,
        findings: &scan.findings,
        baselined: baselined.as_ref(),
        rules: &rules,
        policy: &ws.policy,
    };
    let new = out.new_findings();
    let fails = ws.policy.fails(&new, fail_on);
    let distance = codemosis_core::Distance::from_findings(new.iter().copied());
    let is_baselined = |f: &Finding| baselined.as_ref().is_some_and(|b| b.contains(&f.fingerprint()));
    let mut body = String::new();
    use std::fmt::Write as _;
    match o.format {
        CheckFormat::Json => {
            let v = serde_json::json!({
                "schema": ci::JSON_CHECK_SCHEMA,
                "version": env!("CARGO_PKG_VERSION"),
                "project": root.display().to_string(),
                "distance": distance,
                "fail_on_tripped": fails,
                "findings": scan.findings.iter().map(|f| finding_json(f, &ws.policy, is_baselined(f))).collect::<Vec<_>>(),
                "rule_errors": scan.errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
            });
            body = serde_json::to_string_pretty(&v)? + "\n";
        }
        CheckFormat::Sarif => {
            let root_uri = format!("file://{}/", root.display().to_string().replace('\\', "/"));
            body = serde_json::to_string_pretty(&ci::sarif(&out, Some(&root_uri)))? + "\n";
        }
        CheckFormat::Gitlab => body = serde_json::to_string_pretty(&ci::gitlab(&out))? + "\n",
        CheckFormat::Review => body = serde_json::to_string_pretty(&ci::github_review(&out))? + "\n",
        CheckFormat::Github => {
            body = ci::github_annotations(&out);
            let link = github_link();
            let summary =
                ci::summary_markdown(&out, &distance, link.as_ref().map(|l| l as &dyn Fn(&Finding) -> String));
            if let Some(p) = std::env::var_os("GITHUB_STEP_SUMMARY") {
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&p)
                    .with_context(|| format!("cannot write the job summary to {}", Path::new(&p).display()))?;
                writeln!(f, "{summary}")?;
            }
        }
        CheckFormat::Markdown => {
            body = ci::summary_markdown(&out, &distance, None);
            if o.diff {
                let d = proposed_diff(&ws.project, &new);
                if !d.is_empty() {
                    let _ = write!(body, "\n### Proven corrections (not yet verified)\n\n```diff\n{d}```\n");
                }
            }
        }
        CheckFormat::Text => {
            let _ = writeln!(
                body,
                "{}",
                s.bold(&format!("Reverse Codemosis {}", env!("CARGO_PKG_VERSION")))
            );
            let _ = writeln!(body, "  project   {}", display_root(&root));
            let _ = writeln!(
                body,
                "  packs     {}\n",
                report::packs_line(&applicable(&ws.packs, &ws.project.languages()))
            );
            for c in Category::ALL {
                for f in new.iter().filter(|f| f.category == c) {
                    let _ = writeln!(body, "{}", report::finding(f, &ws.project, s));
                }
            }
            let suppressed: Vec<&Finding> = scan
                .findings
                .iter()
                .filter(|f| matches!(f.status, Status::Suppressed { .. }))
                .collect();
            if o.show_suppressed {
                for f in &suppressed {
                    let _ = writeln!(body, "{}", report::finding(f, &ws.project, s));
                }
            }
            for err in &scan.errors {
                let _ = writeln!(body, "{} {err}", s.red("rule error:"));
            }
            if o.diff {
                let d = proposed_diff(&ws.project, &new);
                if !d.is_empty() {
                    let _ = writeln!(
                        body,
                        "{}\n{}",
                        s.bold("Proven corrections (not yet verified by your build and tests)"),
                        d
                    );
                }
            }
            let fixable = new.iter().filter(|f| f.is_fixable()).count();
            let questions = new
                .iter()
                .filter(|f| matches!(f.status, Status::NeedsAnswer { .. }))
                .count();
            let other = new.len() - fixable - questions;
            let _ = writeln!(body, "{}", s.bold("Summary"));
            let _ = writeln!(body, "  distance  {}", report::distance(&distance, s));
            let _ = writeln!(
                body,
                "  findings  {fixable} provably fixable · {questions} need your decision · {other} explained"
            );
            let in_baseline = baselined.as_ref().map(|b| b.len()).unwrap_or(0);
            if !suppressed.is_empty() || baseline.is_some() {
                let mut parts = Vec::new();
                if !suppressed.is_empty() {
                    parts.push(format!(
                        "{} suppressed{}",
                        suppressed.len(),
                        if o.show_suppressed {
                            ""
                        } else {
                            " (--show-suppressed lists them)"
                        }
                    ));
                }
                if let Some(b) = &baseline {
                    let resolved = b.resolved(&scan.findings).len();
                    let mut text = format!("{in_baseline} accepted by the baseline");
                    if resolved > 0 {
                        text.push_str(&format!(
                            "; {resolved} baseline entr{} fixed since (run `codemosis baseline` to shrink it)",
                            if resolved == 1 { "y is" } else { "ies are" }
                        ));
                    }
                    parts.push(text);
                }
                let _ = writeln!(body, "  hidden    {}", parts.join(" · "));
            }
            if fixable > 0 {
                let _ = writeln!(
                    body,
                    "\nRun `codemosis fix` to apply the provable corrections, verified by your build and tests."
                );
            }
        }
    }
    match o.output {
        Some(p) => {
            std::fs::write(p, &body).with_context(|| format!("cannot write {}", p.display()))?;
            if o.format == CheckFormat::Text {
                eprintln!("Report written to {}", p.display());
            }
        }
        None => print!("{body}"),
    }
    Ok(if fails { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// A link to a finding on GitHub, inside a GitHub Actions run.
fn github_link() -> Option<impl Fn(&Finding) -> String> {
    let server = std::env::var("GITHUB_SERVER_URL").ok()?;
    let repo = std::env::var("GITHUB_REPOSITORY").ok()?;
    let sha = std::env::var("GITHUB_SHA").ok()?;
    Some(move |f: &Finding| {
        format!(
            "{server}/{repo}/blob/{sha}/{}#L{}",
            f.file.to_string_lossy().replace('\\', "/"),
            f.span.start.line
        )
    })
}

fn baseline_cmd(path: &Path, output: &Path) -> Result<ExitCode> {
    use codemosis_engine::policy::Baseline;
    let root = canonical(path)?;
    let ws = Workspace::open(&root, &[])?;
    let scan = ws.scan();
    let b = Baseline::from_findings(&scan.findings, &codemosis_core::suppress::today());
    let rel = output.strip_prefix(&root).unwrap_or(output);
    codemosis_engine::fsguard::write_confined(&root, rel, b.to_json().as_bytes())
        .with_context(|| format!("cannot write {}", output.display()))?;
    let _ = codemosis_engine::audit::record(
        &root,
        "baseline",
        serde_json::json!({"file": rel.display().to_string(), "findings": b.findings.len()}),
    );
    println!(
        "Recorded {} open finding(s) in {}.\n`codemosis check --baseline {}` now fails only on new findings; commit the file.",
        b.findings.len(),
        rel.display(),
        rel.display()
    );
    Ok(ExitCode::SUCCESS)
}

struct FixOptions<'a> {
    dry_run: bool,
    no_verify: bool,
    show_diff: bool,
    report_path: Option<&'a Path>,
    format: Format,
    max_steps: usize,
    interactive: bool,
    proof: bool,
}

fn fix(root: &Path, o: FixOptions, extra: &[PathBuf], only: &[String], s: Style, e: Style) -> Result<ExitCode> {
    let FixOptions {
        dry_run,
        no_verify,
        show_diff,
        report_path,
        format,
        max_steps,
        interactive,
        proof,
    } = o;
    let mut ws = Workspace::open(root, extra)?;
    if interactive {
        // Prompts go to stderr when stdout carries JSON or Markdown.
        let stdin = std::io::stdin();
        if format == Format::Text {
            interactive::ask(&mut ws, only, s, &mut stdin.lock(), &mut std::io::stdout())?;
            println!();
        } else {
            interactive::ask(&mut ws, only, e, &mut stdin.lock(), &mut std::io::stderr())?;
        }
    }
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
    let policy = ws.policy.clone();
    let answers_ledger = ws.answers.clone();
    let answers = |id: &str| answers_ledger.get(id);
    let session = Session {
        project: &mut ws.project,
        rules,
        answers: &answers,
        oracle: oracle.as_ref(),
        policy: Some(&policy),
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
    let mut certificate_path = None;
    if !dry_run && !report.steps.is_empty() {
        let cert = codemosis_engine::proof::Certificate::of(
            &report,
            root,
            &ws.project,
            &ws.packs,
            &ws.answers,
            oracle.as_ref().map(|_| (&section.build, &section.test)),
        );
        if let Some(cert) = cert.filter(|_| proof) {
            let rel = cert.file_name();
            match codemosis_engine::fsguard::write_confined(root, &rel, cert.to_json().as_bytes()) {
                Ok(()) => certificate_path = Some(rel),
                Err(err) => eprintln!("{} cannot write the proof certificate: {err}", e.yellow("warning:")),
            }
        }
        let logged = codemosis_engine::audit::record(
            root,
            "fix",
            serde_json::json!({
                "steps": report.steps.len(),
                "rejected": report.rejected.len(),
                "files": report.files_changed,
                "distance_before": report.distance_before,
                "distance_after": report.distance_after,
                "verified": report.verified,
                "proof": certificate_path.as_ref().map(|p| p.display().to_string()),
            }),
        );
        if let Err(err) = logged {
            eprintln!("{} cannot write the audit log: {err}", e.yellow("warning:"));
        }
    }
    match format {
        Format::Json => println!("{}", fix_json(&report)?),
        Format::Markdown => println!("{}", report::session_markdown(&report, &display_root(root), &shown)),
        Format::Text => print!(
            "{}",
            report::session_text(&report, &ws.project, s, show_diff || dry_run)
        ),
    }
    if let Some(p) = report_path {
        let body = if p.extension().is_some_and(|x| x == "json") {
            fix_json(&report)?
        } else {
            report::session_markdown(&report, &display_root(root), &shown)
        };
        std::fs::write(p, body).with_context(|| format!("cannot write {}", p.display()))?;
        if text {
            println!("Report written to {}", p.display());
        }
    }
    if let (Some(p), true) = (&certificate_path, text) {
        println!(
            "Proof certificate: {}\nAnyone can re-derive this change from it with `codemosis proof verify {}`.",
            p.display(),
            p.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// The fix report as JSON (schema codemosis/fix/v1).
fn fix_json(report: &codemosis_engine::session::Report) -> Result<String> {
    let mut v = serde_json::json!({
        "schema": codemosis_engine::ci::JSON_FIX_SCHEMA,
        "version": env!("CARGO_PKG_VERSION"),
    });
    if let (Some(obj), serde_json::Value::Object(rest)) = (v.as_object_mut(), serde_json::to_value(report)?) {
        obj.extend(rest);
    }
    Ok(serde_json::to_string_pretty(&v)?)
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
    let Some(option) = q.option(answer) else {
        bail!(
            "{} is not an answer to {}; its answers are: {}",
            answer,
            q.id,
            q.choices()
        );
    };
    if let Some(why) = &option.unavailable {
        bail!("{answer} is not possible here: {why}");
    }
    let changes = option.changes;
    let answer = option.answer.clone();
    let answer = answer.as_str();
    ws.answers.set(Answer {
        id: q.id.clone(),
        rule: finding.rule.clone(),
        location: format!("{}:{}", finding.file.display(), finding.span.start.line),
        question: q.text.clone(),
        answer: answer.to_string(),
        note,
    });
    ws.answers.save(&root).map_err(|e| anyhow!(e))?;
    let _ = codemosis_engine::audit::record(
        &root,
        "answer",
        serde_json::json!({"id": q.id, "rule": finding.rule, "answer": answer}),
    );
    println!(
        "Recorded {} to {} in {}.",
        s.bold(answer),
        q.id,
        codemosis_engine::answers::FILE
    );
    if changes {
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
    let sem = spec.semantics.map(|x| x.describe()).unwrap_or("");
    let kind = match (&spec.fix, &spec.ask) {
        (Some(_), None) => format!("corrects automatically ({sem})"),
        (_, Some(ask)) if !ask.options.is_empty() => format!("asks, then corrects as you choose ({sem})"),
        (Some(_), Some(_)) => format!("corrects after you answer yes ({sem})"),
        _ => "explains only".to_string(),
    };
    let target = if rule.languages.is_empty() {
        let langs = rule.pattern_languages();
        if langs.is_empty() {
            "any text file".to_string()
        } else {
            format!("{} languages: {}", langs.len(), langs.join(", "))
        }
    } else {
        rule.languages.iter().map(|l| l.id()).collect::<Vec<_>>().join(", ")
    };
    println!("  category  {} — {}", spec.category, spec.category.describe());
    println!("  language  {target}");
    println!("  action    {kind}");
    if !spec.cwe.is_empty() {
        println!("  weakness  {}", spec.cwe.join(", "));
    }
    if !spec.tags.is_empty() {
        println!("  tags      {}", spec.tags.join(", "));
    }
    if let Some(d) = &spec.deprecated {
        let by = spec
            .replaced_by
            .as_deref()
            .map(|r| format!(" Use {r} instead."))
            .unwrap_or_default();
        println!("  {}  {d}{by}", s.yellow("deprecated"));
    }
    println!();
    println!("{}\n", spec.explain.trim());
    let (title, body) = match (&spec.shape, &spec.pattern, &spec.patterns) {
        (Some(q), _, _) => ("Shape", q.clone()),
        (_, Some(p), _) => ("Pattern", p.clone()),
        (_, _, Some(t)) => ("Patterns", format!("one per language, from the table '{t}'")),
        _ => ("Shape", String::new()),
    };
    println!("{}", s.bold(title));
    for l in body.trim().lines() {
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
        for o in &ask.options {
            println!("    {:<8} {}", o.answer, o.means);
        }
    }
    if let Some(ex) = spec.examples.first() {
        println!("\n{}", s.bold("Example"));
        match &ex.after {
            Some(after) => print!(
                "{}",
                codemosis_engine::session::unified_diff(
                    ex.path
                        .as_deref()
                        .or(rule.languages.first().map(|l| l.example_file()))
                        .unwrap_or("example"),
                    &ex.before,
                    after
                )
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
            p.target()
        );
        for r in &p.rules {
            let kind = match (r.changes_code(), r.asks()) {
                (true, false) => "fix",
                (true, true) => "ask",
                _ => "explain",
            };
            let title = match &r.spec.deprecated {
                Some(_) => format!("{} (deprecated)", r.spec.title),
                None => r.spec.title.clone(),
            };
            println!(
                "  {} {:<7} {:<44} {}",
                s.category(r.spec.category),
                kind,
                r.id,
                s.dim(&title)
            );
        }
        println!();
    }
    Ok(ExitCode::SUCCESS)
}

/// Packs from directories, or the built-in packs when none are given.
fn packs_from(dirs: &[PathBuf]) -> Result<Vec<Pack>> {
    if dirs.is_empty() {
        return pack::builtin().map_err(|e| anyhow!("{e}"));
    }
    let mut v = Vec::new();
    for d in dirs {
        let src = pack::read_dir(d).map_err(|e| anyhow!("{e}"))?;
        v.push(pack::compile(&src).map_err(|e| anyhow!("{e}"))?);
    }
    Ok(v)
}

fn pack_lint(dirs: &[PathBuf], deny_warnings: bool, verbose: bool, s: Style) -> Result<ExitCode> {
    use codemosis_core::lint::{lint_pack, Severity};
    let packs = packs_from(dirs)?;
    let (mut warnings, mut notes) = (0, 0);
    for p in &packs {
        let lints = lint_pack(p);
        let shown: Vec<_> = lints
            .iter()
            .filter(|l| verbose || l.severity == Severity::Warning)
            .collect();
        let mark = if lints.iter().any(|l| l.severity == Severity::Warning) {
            s.yellow("!")
        } else {
            s.green("✓")
        };
        println!("{mark} {} {}", s.bold(&p.meta.id), p.meta.version);
        for l in shown {
            let label = match l.severity {
                Severity::Warning => s.yellow("warning"),
                Severity::Note => s.dim("note"),
            };
            println!("    {label} [{}] {}: {}", l.code, l.at, l.message);
        }
        warnings += lints.iter().filter(|l| l.severity == Severity::Warning).count();
        notes += lints.iter().filter(|l| l.severity == Severity::Note).count();
    }
    println!(
        "\n{} warning(s) · {} note(s){}",
        warnings,
        notes,
        if verbose || notes == 0 { "" } else { " (-v shows notes)" }
    );
    Ok(if deny_warnings && warnings > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn pack_mutations(dirs: &[PathBuf], filter: Option<&str>, verbose: bool, s: Style) -> Result<ExitCode> {
    let packs = packs_from(dirs)?;
    let (mut total, mut survived) = (0, 0);
    for p in &packs {
        println!("{} {}", s.bold(&p.meta.id), p.meta.version);
        for r in &p.rules {
            if filter.is_some_and(|f| !r.id.contains(f)) || r.spec.facts.is_empty() {
                continue;
            }
            let ms = selftest::mutants(r);
            total += ms.len();
            let alive: Vec<_> = ms.iter().filter(|m| m.survived()).collect();
            survived += alive.len();
            let mark = if alive.is_empty() {
                s.green("✓")
            } else {
                s.yellow("!")
            };
            println!(
                "  {mark} {} {}",
                r.id,
                s.dim(&format!(
                    "({} of {} facts proven necessary)",
                    ms.len() - alive.len(),
                    ms.len()
                ))
            );
            for m in &ms {
                match &m.killed_by {
                    None => println!(
                        "      {} without `{}` every check still passes: add a counterexample that needs it",
                        s.yellow("survived"),
                        m.fact
                    ),
                    Some(by) if verbose => println!("      {} `{}` (by: {by})", s.green("killed"), m.fact),
                    Some(_) => {}
                }
            }
        }
    }
    println!(
        "\n{} of {} facts are proven necessary by an example or counterexample · {} unproven",
        total - survived,
        total,
        survived
    );
    Ok(ExitCode::SUCCESS)
}

fn proof_verify(certificate: &Path, path: &Path, s: Style) -> Result<ExitCode> {
    use codemosis_engine::proof::{verify, Certificate};
    let root = canonical(path)?;
    let cert_path = if certificate.is_relative() && !certificate.exists() {
        root.join(certificate)
    } else {
        certificate.to_path_buf()
    };
    let cert = Certificate::load(&cert_path).map_err(|e| anyhow!(e))?;
    let (cfg, _) = config::Config::load(&root)?;
    let packs = cfg.load_packs(&root, &[])?;
    let mut project = codemosis_core::Project::load(&root, &cfg.filter())?;
    println!(
        "{} {} ({} step(s), {} file(s), engine {}, created {})",
        s.bold("Verifying"),
        cert_path.display(),
        cert.steps.len(),
        cert.files.len(),
        cert.engine.version,
        cert.created
    );
    let v = verify(&cert, &root, &mut project, &packs);
    for (name, r) in &v.checks {
        match r {
            Ok(()) => println!("  {} {name}", s.green("✓")),
            Err(why) => println!("  {} {name}: {why}", s.red("✗")),
        }
    }
    if v.ok() {
        let answers = if cert.answers.is_empty() {
            String::new()
        } else {
            format!(", given {} recorded answer(s)", cert.answers.len())
        };
        println!("\nVerified: the change is exactly what the pinned rules prove{answers}.");
        if let Some(o) = &cert.oracle {
            if let (Some(b), Some(a)) = (&o.before, &o.after) {
                println!(
                    "Recorded oracle: tests passing {} -> {}, failing {} -> {} (re-run your test suite to confirm it).",
                    b.passed, a.passed, b.failed, a.failed
                );
            }
        }
        Ok(ExitCode::SUCCESS)
    } else {
        println!("\nNot verified.");
        Ok(ExitCode::from(1))
    }
}

fn lock_cmd(path: &Path, check: bool) -> Result<ExitCode> {
    use codemosis_engine::lock::{Lock, LOCK_FILE};
    let root = canonical(path)?;
    let (cfg, _) = config::Config::load(&root)?;
    let packs = cfg.load_packs(&root, &[])?;
    let fresh = Lock::of(&packs);
    if check {
        return match Lock::load(&root).map_err(|e| anyhow!(e))? {
            None => bail!(
                "{} has no {LOCK_FILE}; run `codemosis lock` to create it",
                root.display()
            ),
            Some(l) => {
                let problems = l.verify(&packs);
                if problems.is_empty() {
                    println!("{LOCK_FILE} matches all {} pack(s).", packs.len());
                    Ok(ExitCode::SUCCESS)
                } else {
                    for p in problems {
                        println!("  - {p}");
                    }
                    Ok(ExitCode::from(1))
                }
            }
        };
    }
    codemosis_engine::fsguard::write_confined(&root, Path::new(LOCK_FILE), fresh.to_toml().as_bytes())?;
    let _ = codemosis_engine::audit::record(
        &root,
        "lock",
        serde_json::json!({"packs": fresh.packs.iter().map(|p| format!("{}@{}", p.id, &p.sha256[..12])).collect::<Vec<_>>()}),
    );
    println!("Pinned {} pack(s) in {LOCK_FILE}. Commit it.", fresh.packs.len());
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
    let mut seen = std::collections::BTreeSet::new();
    for l in lang::all() {
        if language.is_some_and(|x| x != l.id()) {
            continue;
        }
        if l.facts().is_empty() && l.filters().is_empty() {
            continue;
        }
        if !seen.insert(l.prefix()) {
            continue;
        }
        let sharing: Vec<&str> = lang::all()
            .iter()
            .filter(|o| o.prefix() == l.prefix())
            .map(|o| o.id())
            .collect();
        println!(
            "\n{}",
            s.bold(&format!("{} facts ({}.*)", sharing.join(", "), l.prefix()))
        );
        for f in l.facts() {
            let sig = f.signature.replace("<prefix>", l.prefix());
            println!("  {:<46} {}", sig, s.dim(f.doc));
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
