//! `codemosis doctor`: is everything Reverse Codemosis relies on in place?
//!
//! Checks the engine, the project, its configuration, packs, lockfile,
//! answers and baseline, the build and test commands, and the toolchains
//! pack examples run with. Each problem says what to do about it.

use codemosis_core::linguist::Confidence;
use codemosis_core::{Project, Tier};
use codemosis_engine::report::Style;
use codemosis_engine::{answers::Answers, config, lock, policy, vcs};
use std::path::Path;
use std::process::Command;

#[derive(PartialEq, Eq, Clone, Copy)]
enum Mark {
    Ok,
    Warn,
    Fail,
    Info,
}

struct Report {
    lines: Vec<(Mark, String, String)>,
}

impl Report {
    fn add(&mut self, mark: Mark, what: impl Into<String>, detail: impl Into<String>) {
        self.lines.push((mark, what.into(), detail.into()));
    }
}

fn version_of(program: &str, arg: &str) -> Option<String> {
    let out = Command::new(program).arg(arg).output().ok()?;
    let text = if out.stdout.is_empty() { out.stderr } else { out.stdout };
    let first = String::from_utf8_lossy(&text).lines().next()?.trim().to_string();
    (!first.is_empty()).then_some(first)
}

fn on_path(program: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        let p = dir.join(program);
        p.is_file() || (cfg!(windows) && dir.join(format!("{program}.exe")).is_file())
    })
}

/// The executable a shell command starts with, skipping `VAR=value` prefixes.
fn command_program(cmd: &str) -> Option<&str> {
    cmd.split_whitespace().find(|w| !w.contains('='))
}

pub fn run(root: &Path, s: Style) -> bool {
    let mut r = Report { lines: Vec::new() };

    // The engine itself.
    r.add(
        Mark::Info,
        "engine",
        format!(
            "codemosis {} on {}/{}, {} thread(s), tree-sitter {}",
            codemosis_core::version::ENGINE_VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
            codemosis_core::parallel::threads(),
            codemosis_core::lang::runtime_version().unwrap_or("?")
        ),
    );

    // Version control.
    match vcs::state(root) {
        vcs::TreeState::Clean => r.add(Mark::Ok, "git", "clean working tree; `codemosis fix` can run"),
        vcs::TreeState::Dirty(p) => r.add(
            Mark::Warn,
            "git",
            format!(
                "{} uncommitted change(s); commit or stash before `codemosis fix`",
                p.len()
            ),
        ),
        vcs::TreeState::NotARepository => r.add(
            Mark::Warn,
            "git",
            "not a git repository; corrections could not be reviewed with `git diff` (fix needs --allow-no-vcs)",
        ),
        vcs::TreeState::Unknown => r.add(Mark::Warn, "git", "git is not installed"),
    }

    // Configuration and policy.
    let cfg = match config::Config::load(root) {
        Ok((c, path)) => {
            r.add(
                Mark::Ok,
                "configuration",
                path.map(|p| format!("{} is valid", p.display()))
                    .unwrap_or_else(|| "no codemosis.toml; defaults apply (`codemosis init` writes one)".into()),
            );
            Some(c)
        }
        Err(e) => {
            r.add(Mark::Fail, "configuration", e.to_string());
            None
        }
    };
    if let Some(c) = &cfg {
        match c.policy() {
            Ok(_) => r.add(
                Mark::Ok,
                "policy",
                "suppressions, severities and the gate threshold are valid",
            ),
            Err(e) => r.add(Mark::Fail, "policy", e.to_string()),
        }
    }

    // Packs, compiled one by one so each problem is attributed.
    if let Some(c) = &cfg {
        match c.pack_sources(root, &[]) {
            Err(e) => r.add(Mark::Fail, "packs", e.to_string()),
            Ok(sources) => {
                let results = codemosis_core::parallel::map(&sources, codemosis_core::pack::compile);
                let mut rules = 0;
                let mut bad = 0;
                for res in &results {
                    match res {
                        Ok(p) => rules += p.rules.len(),
                        Err(e) => {
                            bad += 1;
                            r.add(Mark::Fail, "pack", e.to_string().trim().to_string());
                        }
                    }
                }
                if bad == 0 {
                    r.add(
                        Mark::Ok,
                        "packs",
                        format!("{} pack(s), {rules} rule(s), all compile", results.len()),
                    );
                }
                match lock::Lock::load(root) {
                    Ok(None) => r.add(
                        Mark::Info,
                        "lockfile",
                        "none; `codemosis lock` pins the packs so a pack change is a reviewed change",
                    ),
                    Ok(Some(l)) => {
                        let ids: Vec<(String, String, String)> = sources
                            .iter()
                            .filter_map(codemosis_core::pack::index)
                            .map(|i| (i.id, i.version, i.digest))
                            .collect();
                        let problems = l.verify_ids(&ids);
                        if problems.is_empty() {
                            r.add(
                                Mark::Ok,
                                "lockfile",
                                format!("all {} pinned pack(s) match", l.packs.len()),
                            );
                        } else {
                            for p in problems {
                                r.add(Mark::Fail, "lockfile", p);
                            }
                        }
                    }
                    Err(e) => r.add(Mark::Fail, "lockfile", e),
                }
            }
        }
    }

    // Recorded decisions.
    match Answers::load(root) {
        Ok(a) if a.entries.is_empty() => r.add(Mark::Info, "answers", "no recorded answers yet"),
        Ok(a) => r.add(
            Mark::Ok,
            "answers",
            format!("{} recorded answer(s) parse", a.entries.len()),
        ),
        Err(e) => r.add(Mark::Fail, "answers", e),
    }
    if let Some(b) = cfg.as_ref().and_then(|c| c.policy.baseline.clone()) {
        match policy::Baseline::load(&root.join(&b)) {
            Ok(base) => r.add(
                Mark::Ok,
                "baseline",
                format!("{b}: {} accepted finding(s)", base.findings.len()),
            ),
            Err(e) => r.add(Mark::Fail, "baseline", e),
        }
    }
    let proofs = std::fs::read_dir(root.join(codemosis_engine::proof::PROOF_DIR))
        .map(|d| d.flatten().count())
        .unwrap_or(0);
    if proofs > 0 {
        r.add(
            Mark::Info,
            "proofs",
            format!("{proofs} certificate(s) in {}", codemosis_engine::proof::PROOF_DIR),
        );
    }

    // The project as the engine reads it.
    let filter = cfg.as_ref().map(|c| c.filter()).unwrap_or_default();
    match Project::load(root, &filter) {
        Err(e) => r.add(Mark::Fail, "project", e.to_string()),
        Ok(p) => {
            let count = |t: Tier| p.files.iter().filter(|f| f.tier() == t).count();
            r.add(
                Mark::Ok,
                "project",
                format!(
                    "{} file(s): {} deep · {} lexical · {} recognized · {} not read",
                    p.files.len(),
                    count(Tier::Deep),
                    count(Tier::Lexical),
                    count(Tier::Recognized),
                    p.skipped.len()
                ),
            );
            let guesses: Vec<String> = p
                .files
                .iter()
                .filter(|f| f.detection.confidence == Confidence::Guess)
                .map(|f| f.display_path())
                .collect();
            if !guesses.is_empty() {
                r.add(
                    Mark::Warn,
                    "detection",
                    format!(
                        "{} file(s) whose language is only a guess get no rules: {}{}",
                        guesses.len(),
                        guesses.iter().take(3).cloned().collect::<Vec<_>>().join(", "),
                        if guesses.len() > 3 { ", ..." } else { "" }
                    ),
                );
            }
            let section = cfg.as_ref().map(|c| c.format.clone()).unwrap_or_default();
            let formatters = codemosis_engine::formatter::Formatters::new(root, &section, false);
            let survey = formatters.survey(p.files.iter().filter_map(|f| Some((f.path.as_path(), f.lang?.id()))));
            if section.mode == codemosis_engine::formatter::Mode::Off {
                r.add(
                    Mark::Info,
                    "formatting",
                    "[format] mode = \"off\": corrections are not held to a formatter",
                );
            }
            for (g, n) in survey
                .iter()
                .filter(|_| section.mode != codemosis_engine::formatter::Mode::Off)
            {
                let what = format!("formatter {}", g.kind);
                match formatters.tool(g.kind) {
                    Ok(t) => r.add(
                        Mark::Ok,
                        what,
                        format!(
                            "{} ({}) governs {n} file(s); corrections keep formatted files formatted ({})",
                            t.version,
                            g.evidence,
                            match section.mode {
                                codemosis_engine::formatter::Mode::Reformat => "reformat",
                                _ => "refuse",
                            }
                        ),
                    ),
                    Err(why) if g.kind.runs_project_code() => {
                        r.add(Mark::Info, what, format!("{} governs {n} file(s); {why}", g.evidence))
                    }
                    Err(why) => r.add(
                        Mark::Warn,
                        what,
                        format!(
                            "{} governs {n} file(s), but {why}; formatting is not checked",
                            g.evidence
                        ),
                    ),
                }
            }
            let damaged = codemosis_engine::ci::parse_hazards(&p);
            if !damaged.is_empty() {
                r.add(
                    Mark::Warn,
                    "parsing",
                    format!(
                        "{} file(s) have code the grammar could not parse; no correction is made there (first: {}:{})",
                        damaged.len(),
                        damaged[0].0,
                        damaged[0].1
                    ),
                );
            }
        }
    }

    // The oracle.
    let section = cfg
        .as_ref()
        .filter(|c| c.oracle.build.is_some() || c.oracle.test.is_some())
        .map(|c| (c.oracle.clone(), false))
        .unwrap_or_else(|| (config::detect_oracle(root), true));
    let (o, detected) = section;
    if o.build.is_none() && o.test.is_none() {
        r.add(
            Mark::Warn,
            "oracle",
            "no build or test command found; `fix` can only check structure. Set [oracle] in codemosis.toml",
        );
    }
    for (name, cmd) in [("build", &o.build), ("test", &o.test)] {
        let Some(cmd) = cmd else { continue };
        let program = command_program(cmd).unwrap_or("");
        let found = on_path(program) || Path::new(program).is_file() || root.join(program).is_file();
        r.add(
            if found { Mark::Ok } else { Mark::Fail },
            format!("oracle {name}"),
            format!(
                "`{cmd}`{}{}",
                if detected { " (detected)" } else { "" },
                if found {
                    String::new()
                } else {
                    format!(": `{program}` is not on PATH")
                }
            ),
        );
    }

    // Toolchains that run pack examples.
    for (program, arg, langs) in [
        ("python3", "--version", "Python"),
        ("rustc", "--version", "Rust"),
        ("node", "--version", "JavaScript, TypeScript (22.6+ for type stripping)"),
        ("go", "version", "Go"),
        ("javac", "-version", "Java"),
        ("cc", "--version", "C"),
        ("c++", "--version", "C++"),
        ("ruby", "--version", "Ruby"),
        ("php", "--version", "PHP"),
        ("bash", "--version", "Bash"),
        ("lua", "-v", "Lua"),
        ("swift", "--version", "Swift"),
    ] {
        match version_of(program, arg) {
            Some(v) => r.add(
                Mark::Ok,
                format!("runner {program}"),
                format!("{v} — runs {langs} examples"),
            ),
            None => r.add(
                Mark::Info,
                format!("runner {program}"),
                format!("not found; {langs} examples are checked without running (reported as skipped)"),
            ),
        }
    }

    let width = r.lines.iter().map(|(_, w, _)| w.len()).max().unwrap_or(10);
    for (mark, what, detail) in &r.lines {
        let m = match mark {
            Mark::Ok => s.green("✓"),
            Mark::Warn => s.yellow("!"),
            Mark::Fail => s.red("✗"),
            Mark::Info => s.dim("·"),
        };
        println!("{m} {what:<width$}  {detail}");
    }
    let fails = r.lines.iter().filter(|(m, _, _)| *m == Mark::Fail).count();
    let warns = r.lines.iter().filter(|(m, _, _)| *m == Mark::Warn).count();
    println!("\n{fails} problem(s) · {warns} warning(s)");
    fails == 0
}
