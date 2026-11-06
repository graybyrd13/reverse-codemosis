# Editors

`codemosis lsp` is a language server, and every editor below runs it the same way. It gives you:

- findings as you type;
- proven corrections as quick fixes, and "fix all" (`source.fixAll.codemosis`) for a whole file;
- questions answered with code actions (keep or remove a fixed wait, say), recorded in
  `.codemosis/answers.toml` exactly as `codemosis answer` would;
- the rule's explanation on hover.

The language server never runs your build or tests and never writes files on its own. `codemosis fix`
remains the command that verifies corrections. Every editor needs the `codemosis` executable:

```console
$ cargo install --locked --git https://github.com/graybyrd13/reverse-codemosis codemosis-cli
```

| Editor | What there is | Where |
|---|---|---|
| [VS Code](#vs-code) | an extension: language server, MCP server for Copilot, commands, schema validation | [`editors/vscode`](../editors/vscode) |
| [Zed](#zed) | an extension: language server and MCP context server | [`editors/zed`](../editors/zed) |
| [Neovim](#neovim) | a plugin: language server, commands, quickfix | [`editors/neovim`](../editors/neovim) |
| [Vim](#vim) | `:make` and the quickfix list | below |
| [Helix](#helix) | `languages.toml` | [`editors/helix`](../editors/helix) |
| [Emacs](#emacs) | `codemosis.el`: lsp-mode add-on, eglot, compilation buffer | [`editors/emacs`](../editors/emacs) |
| [Sublime Text](#sublime-text) | an LSP client entry | [`editors/sublime`](../editors/sublime) |
| [JetBrains IDEs](#jetbrains-ides) | LSP4IJ, and the AI Assistant's MCP settings | below |

## VS Code

The extension in [`editors/vscode`](../editors/vscode):

- starts the language server for every file;
- offers `codemosis mcp` to Copilot's agent mode and other MCP clients in VS Code (version 1.101 or later), so
  agents can check code, apply proven corrections and bring questions back to you;
- adds commands under "Reverse Codemosis" in the Command Palette: check the project or what changed,
  preview or apply the corrections (verified by your build and tests), answer the open questions, and map
  the project;
- validates `codemosis.toml`, `codemosis.lock` and `pack.toml` (with a TOML extension that reads schemas,
  such as Even Better TOML), proof certificates and baselines.

Build and install it from the repository:

```console
$ cd editors/vscode
$ npm ci
$ npm run package                 # writes codemosis.vsix
$ code --install-extension codemosis.vsix
```

The settings are `codemosis.path`, `codemosis.languageServer.enable`, `codemosis.mcp.enable` and
`codemosis.trace.server`. Cursor, Windsurf and VSCodium install the same `.vsix`.

## Zed

The extension in [`editors/zed`](../editors/zed) starts the language server for the languages it lists, and
registers `codemosis mcp` as a context server for Zed's agent. Install it with **zed: install dev extension**,
choosing the `editors/zed` directory; Zed builds it. To add it to a language that is not listed:

```json
{
  "languages": {
    "Elixir": { "language_servers": ["elixir-ls", "codemosis"] }
  }
}
```

## Neovim

The plugin in [`editors/neovim`](../editors/neovim) needs Neovim 0.10 or later (0.11 or later uses
`vim.lsp.enable`). With lazy.nvim:

```lua
{
  dir = "/path/to/reverse-codemosis/editors/neovim",
  config = function() require("codemosis").setup() end,
}
```

It starts the language server for every buffer, and adds `:Codemosis check|changed|preview|fix|ask|map`
(each runs in a terminal) and `:CodemosisQuickfix [changed]`, which loads the findings into the quickfix
list. On Neovim 0.11 or later without the plugin, put [`lsp/codemosis.lua`](../editors/neovim/lsp/codemosis.lua)
on your runtime path and call `vim.lsp.enable("codemosis")`.

## Vim

`--format compact` prints `path:line:col: level: message [rule; status]`, which `:make` understands:

```vim
set makeprg=codemosis\ check\ --format\ compact\ --fail-on\ never
set errorformat=%f:%l:%c:\ %trror:\ %m,%f:%l:%c:\ %tarning:\ %m,%f:%l:%c:\ %tote:\ %m
```

`:make` then fills the quickfix list. For findings as you type, run `codemosis lsp` with an LSP plugin such
as vim-lsp or coc.nvim.

## Helix

Add [`editors/helix/languages.toml`](../editors/helix/languages.toml) to `~/.config/helix/languages.toml`.
Helix replaces a language's default servers when you list them, so keep the ones you use (`hx --health
python` shows them) and add `"codemosis"`.

## Emacs

[`editors/emacs/codemosis.el`](../editors/emacs/codemosis.el), loaded with `(require 'codemosis)`:

- **lsp-mode**: registers `codemosis lsp` as an add-on server, next to your other language servers.
- **eglot**, which runs one server per buffer: `M-x codemosis-eglot-setup` makes Reverse Codemosis the server
  for the modes in `codemosis-eglot-modes`.
- `M-x codemosis-check` (with a prefix argument, only changed files) lists findings in a compilation buffer, so
  `next-error` visits them; `M-x codemosis-fix` applies the proven corrections.

## Sublime Text

With the LSP package, add the client in [`editors/sublime/LSP.sublime-settings`](../editors/sublime/LSP.sublime-settings)
to Preferences > Package Settings > LSP > Settings.

## JetBrains IDEs

IntelliJ IDEA, PyCharm, GoLand, WebStorm and the others run language servers through the
[LSP4IJ](https://plugins.jetbrains.com/plugin/23257-lsp4ij) plugin:

1. Settings > Languages & Frameworks > Language Servers, then **+**.
2. Name: `Reverse Codemosis`; Command: `codemosis lsp`.
3. Mappings > File name patterns: `*` (or the file types you want).

For the AI Assistant's agent, add an MCP server under Settings > Tools > AI Assistant > Model Context Protocol
with the command `codemosis mcp`. See [agent tools](AGENT_TOOLS.md).
