# Integrations for agentic coding tools

Reverse Codemosis reaches agents three ways:

| | What it is | Best for |
|---|---|---|
| **MCP server** (`codemosis mcp`) | `check`, `fix`, `answer`, `explain`, `map`, `rules` and `verify_proof` as tools | any tool that speaks MCP |
| **Hook** (`codemosis hook`) | after each edit, reports findings on the lines the agent changed, with what to do | Claude Code, and tools whose hooks pass JSON on stdin |
| **CLI** (`codemosis check --changed --format compact`) | one line per finding | Aider's lint command, scripts, everything else |

`codemosis integrate <tool>` writes a tool's project configuration for you (merging into existing files);
`codemosis integrate` lists the tools. What is here:

- [`claude-code/`](claude-code): a Claude Code plugin: the MCP server, the hook, a skill and the
  `/codemosis-check` and `/codemosis-fix` commands. Install it from this repository's marketplace:

  ```text
  /plugin marketplace add graybyrd13/reverse-codemosis
  /plugin install codemosis@reverse-codemosis
  ```

  Or, without the plugin: `codemosis integrate claude-code`.

The [agent tools guide](../docs/AGENT_TOOLS.md) has the setup for each tool, and what agents should and
should not do with the questions Reverse Codemosis asks.
