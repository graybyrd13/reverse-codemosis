//! Rust: grammar and a snippet runner built on `rustc`.

use super::{Language, RunOutcome, RunStatus, Runner};
use crate::process::{self, ScratchDir};
use std::process::Command;
use std::time::Duration;
use tree_sitter::Node;

pub struct Rust;

impl Language for Rust {
    fn id(&self) -> &'static str {
        "rust"
    }

    fn prefix(&self) -> &'static str {
        "rs"
    }

    fn linguist_names(&self) -> &'static [&'static str] {
        &["Rust"]
    }

    fn definition_kinds(&self) -> &'static [(&'static str, &'static str)] {
        &[
            ("function_item", "name"),
            ("struct_item", "name"),
            ("enum_item", "name"),
            ("trait_item", "name"),
            ("mod_item", "name"),
            ("impl_item", "type"),
        ]
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn example_file(&self) -> &'static str {
        "example.rs"
    }

    fn is_comment(&self, node: &Node) -> bool {
        matches!(node.kind(), "line_comment" | "block_comment")
    }

    fn definition_name(&self, node: &Node, text: &str) -> Option<String> {
        let field = match node.kind() {
            "function_item" | "struct_item" | "enum_item" | "trait_item" | "mod_item" => "name",
            "impl_item" => "type",
            _ => return None,
        };
        node.child_by_field_name(field)
            .map(|n| text[n.byte_range()].to_string())
    }

    fn runner(&self) -> Option<&dyn Runner> {
        Some(&RustRunner)
    }
}

/// Compiles snippets with `rustc` (or `$CODEMOSIS_RUSTC`) and runs those
/// that have a `main`.
pub struct RustRunner;

fn rustc() -> String {
    std::env::var("CODEMOSIS_RUSTC").unwrap_or_else(|_| "rustc".to_string())
}

impl RustRunner {
    fn exec(&self, code: &str, run: bool) -> RunOutcome {
        let dir = match ScratchDir::new("rs") {
            Ok(d) => d,
            Err(e) => return RunOutcome::unavailable(format!("cannot create a scratch directory: {e}")),
        };
        let src = dir.path.join("snippet.rs");
        if let Err(e) = std::fs::write(&src, code) {
            return RunOutcome::unavailable(format!("cannot write snippet: {e}"));
        }
        let has_main = code.contains("fn main");
        let bin = dir.path.join(if cfg!(windows) { "snippet.exe" } else { "snippet" });
        let mut cmd = Command::new(rustc());
        cmd.arg("--edition").arg("2021").arg("-A").arg("warnings");
        if has_main && run {
            cmd.arg("-o").arg(&bin);
        } else {
            cmd.arg("--crate-type")
                .arg(if has_main { "bin" } else { "lib" })
                .arg("--emit=metadata")
                .arg("--out-dir")
                .arg(&dir.path);
        }
        cmd.arg(&src).current_dir(&dir.path);
        let compiled = match process::run(&mut cmd, Duration::from_secs(120)) {
            Err(e) => return RunOutcome::unavailable(format!("cannot run {}: {e}", rustc())),
            Ok(r) => r,
        };
        if !compiled.success() {
            return RunOutcome {
                status: if compiled.timed_out {
                    RunStatus::TimedOut
                } else {
                    RunStatus::CompileError
                },
                stdout: compiled.stdout,
                stderr: compiled.stderr,
            };
        }
        if !(run && has_main) {
            return RunOutcome {
                status: RunStatus::Ok,
                stdout: String::new(),
                stderr: String::new(),
            };
        }
        match process::run(Command::new(&bin).current_dir(&dir.path), Duration::from_secs(20)) {
            Err(e) => RunOutcome::unavailable(format!("cannot run the compiled snippet: {e}")),
            Ok(r) => RunOutcome {
                status: if r.timed_out {
                    RunStatus::TimedOut
                } else if r.success() {
                    RunStatus::Ok
                } else {
                    RunStatus::Failed(r.code)
                },
                stdout: r.stdout,
                stderr: r.stderr,
            },
        }
    }
}

impl Runner for RustRunner {
    fn describe(&self) -> String {
        rustc()
    }

    fn compile(&self, code: &str) -> RunOutcome {
        self.exec(code, false)
    }

    fn run(&self, code: &str) -> RunOutcome {
        self.exec(code, true)
    }
}
