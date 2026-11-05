//! Reverse Codemosis for Zed: starts `codemosis lsp` for every language it
//! reads (findings, proven corrections as code actions, questions, hover),
//! and `codemosis mcp` as a context server for Zed's agent.

use zed_extension_api::{self as zed, Command, ContextServerId, LanguageServerId, Project, Result, Worktree};

const INSTALL: &str = "Reverse Codemosis needs the `codemosis` executable on PATH: \
     cargo install --locked --git https://github.com/graybyrd13/reverse-codemosis codemosis-cli";

struct Codemosis;

impl zed::Extension for Codemosis {
    fn new() -> Self {
        Codemosis
    }

    fn language_server_command(&mut self, _id: &LanguageServerId, worktree: &Worktree) -> Result<Command> {
        let path = worktree.which("codemosis").ok_or_else(|| INSTALL.to_string())?;
        Ok(Command {
            command: path,
            args: vec!["lsp".into()],
            env: worktree.shell_env(),
        })
    }

    fn context_server_command(&mut self, _id: &ContextServerId, _project: &Project) -> Result<Command> {
        // Context servers get no worktree to search; Zed resolves the name on PATH.
        Ok(Command {
            command: "codemosis".into(),
            args: vec!["mcp".into()],
            env: Vec::new(),
        })
    }
}

zed::register_extension!(Codemosis);
