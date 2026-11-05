# Agentic coding tools

Coding agents write a lot of almost-right code. Reverse Codemosis gives them what it gives people: findings
it can prove, corrections that are verified, and questions it will not answer for you. This guide connects
it to the agent you use.

## Three ways in

| | Command | What the agent gets |
|---|---|---|
| **MCP server** | `codemosis mcp` | Tools: `check`, `fix`, `answer`, `explain`, `map`, `rules`, `verify_proof` |
| **Hook** | `codemosis hook` | After each edit: the findings on the lines it just changed, and what to do about each |
| **CLI** | `codemosis check --changed --format compact` | One line per finding, for lint commands and scripts |

The quickest setup is `codemosis integrate <tool>`, which writes the tool's project configuration. It merges
into existing files and never replaces them; run `codemosis integrate` to list the tools. `codemosis` must
be on `PATH` (`cargo install --locked --path crates/codemosis-cli`).

## What agents should do with findings

Every finding is one of three kinds, and the MCP server's instructions, the hook's report and the
[skill](../integrations/claude-code/skills/reverse-codemosis/SKILL.md) all tell the agent the same thing:

- **Proven correction.** Safe to apply exactly as shown, or with `fix`, which keeps each step only if the
  build and tests do not regress and writes a [proof certificate](SEMANTICS.md#proof-certificates).
- **Question.** The user's decision: keep or remove a fixed wait, which side of a merge conflict. The agent
  asks you and records *your* answer with `answer`. The tool's description says to never choose for you,
  and answers land in `.codemosis/answers.toml`, which you review and commit like code.
- **Explained.** A real risk with no provable correction. The agent tells you, rather than guessing.

## The MCP server

`codemosis mcp` speaks the Model Context Protocol over standard input and output (protocol versions
2025-06-18, 2025-03-26 and 2024-11-05). It serves the directory it starts in, or `--path DIR`.

| Tool | Does | Changes files |
|---|---|---|
| `check` | Findings for the project, some `files`, or what `changed` since the last commit (`changed_lines_only` for just the changed lines), as an agent-ready report and as structured content | no |
| `fix` | Previews the proven corrections (default); `apply=true` writes them, verifying each step with the build and tests (`verify`, default true), and writes a certificate. Needs a clean git tree unless `allow_dirty=true` | with `apply` |
| `answer` | Records the user's answer to a question (`question_id`, `answer`, optional `note`) | `.codemosis/answers.toml` |
| `explain` | What a rule finds, why, its facts and its proof | no |
| `map` | Languages and how deeply each is understood, build and tests, formatters | no |
| `rules` | The rules, optionally filtered by a `query` | no |
| `verify_proof` | Replays a proof certificate | no |

`fix` with `apply=true` runs the project's build and tests, as `codemosis fix` does: only point agents at
projects whose tests you would run yourself.

## The hook

`codemosis hook` reads the hook payload a tool sends on standard input and checks the files it names, or
every changed file for a hook without file paths, such as one that runs when the agent stops. It reports only
findings on lines that differ from the last commit, so the agent hears about its own code and not about
old issues elsewhere in the file. With findings it writes the report to standard error and exits 2, which
Claude Code feeds back to the model. Without findings it prints nothing and exits 0. Options: `--all-lines`,
`--exit-code N`, and `--json` for structured output.

A report looks like this:

```text
Reverse Codemosis found 2 issues in the code you just changed.

app/core.py:11:8 python-core/is-literal (proven correction, corrected)
  `len(items) is 0` asks whether two objects are the same object in memory, not whether they are equal.
  The correction:
    -    if len(items) is 0:
    +    if len(items) == 0:
  Apply exactly this change, or run: codemosis fix --rule python-core/is-literal --allow-dirty

app/core.py:13:5 python-core/fixed-wait (a decision for the user)
  `time.sleep(2)` pauses for a fixed time instead of waiting for what the code actually needs.
  Ask the user: Keep this fixed wait?
    - keep: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
    - remove: delete it; if something must be ready first, wait for that condition instead
  Then record their answer: codemosis answer Q-0e614498 <answer>. Do not choose for them.
```

## Tool by tool

### Claude Code

The plugin in [`integrations/claude-code`](../integrations/claude-code) bundles the MCP server, the hook
(after every `Edit`, `MultiEdit` and `Write`), a skill, and the `/codemosis-check` and `/codemosis-fix`
commands:

```text
/plugin marketplace add graybyrd13/reverse-codemosis
/plugin install codemosis@reverse-codemosis
```

Without the plugin, `codemosis integrate claude-code` writes `.mcp.json` and the hook into
`.claude/settings.json`; or `claude mcp add codemosis -- codemosis mcp`.

### Cursor

`codemosis integrate cursor` writes `.cursor/mcp.json` and a project rule, `.cursor/rules/codemosis.mdc`,
that tells the agent how to treat findings.

### VS Code and GitHub Copilot

The [VS Code extension](EDITORS.md#vs-code) registers the MCP server for Copilot's agent mode by itself. Without
it, `codemosis integrate vscode` writes `.vscode/mcp.json`.

### OpenAI Codex CLI

Add to `~/.codex/config.toml` (`codemosis integrate codex` prints it):

```toml
[mcp_servers.codemosis]
command = "codemosis"
args = ["mcp"]
```

Codex reads `AGENTS.md`: `codemosis integrate agents-md` appends a section on how to use Reverse Codemosis.

### Gemini CLI, Zed, Continue, opencode

`codemosis integrate gemini` (`.gemini/settings.json`), `zed` (`.zed/settings.json`, as a context server),
`continue` (`.continue/mcpServers/codemosis.yaml`) and `opencode` (`opencode.json`).

### Windsurf and Cline

These are configured per user: `codemosis integrate windsurf` and `codemosis integrate cline` print the entry
to add to their MCP settings.

### Aider

`codemosis integrate aider` makes Reverse Codemosis Aider's lint command (`.aider.conf.yml`). Aider runs it on
every file it edits and offers to fix what it reports:

```yaml
lint-cmd:
  - "codemosis check --format compact --fail-on any --file"
auto-lint: true
```

### Any other agent

`codemosis integrate agents-md` adds guidance to `AGENTS.md`, which most agents read. Any tool that can run a
command after an edit can run `codemosis hook` or `codemosis check --file <path> --format compact`.
