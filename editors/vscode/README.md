# Reverse Codemosis for VS Code

[Reverse Codemosis](https://github.com/graybyrd13/reverse-codemosis) finds where code falls short, applies
only the corrections it can prove (each checked against your build and tests), and asks you when a decision is
yours. It has no AI model inside and never guesses. It covers every language GitHub knows.

## What the extension does

- **Findings as you type**, from `codemosis lsp`: proven corrections, questions, and risks to know about.
- **Quick fixes** for proven corrections, and **Fix all** (`source.fixAll.codemosis`) for a whole file.
- **Questions answered from the editor**: "Keep this fixed wait?" offers *keep* and *remove* as code
  actions. Your answer is recorded in `.codemosis/answers.toml`.
- **Hover** over a finding for the rule's explanation.
- **Agents**: the `codemosis mcp` server is offered to Copilot's agent mode and other MCP clients in VS Code.
  Agents can then check code, apply proven corrections and bring the questions back to you.
- **Commands** (Command Palette, "Reverse Codemosis"): check the project or what changed, preview or apply
  the proven corrections (verified by your build and tests), answer the open questions, map the project.
- **Validation** of `codemosis.toml`, `codemosis.lock`, `pack.toml` (with a TOML extension that reads
  schemas, such as Even Better TOML), proof certificates and baselines.

## Requirements

The `codemosis` executable, on `PATH` or set in `codemosis.path`:

```console
$ cargo install --locked --git https://github.com/graybyrd13/reverse-codemosis codemosis-cli
```

## Settings

| Setting | Default | |
|---|---|---|
| `codemosis.path` | `codemosis` | the executable |
| `codemosis.languageServer.enable` | `true` | findings as you type |
| `codemosis.mcp.enable` | `true` | offer the MCP server to agents |
| `codemosis.trace.server` | `off` | trace the language server's messages |

The language server never runs your build or tests and never writes files on its own. `codemosis fix` (the
"Apply the proven corrections" command) is what verifies corrections.
