# Editor integrations

Every editor runs the same language server, `codemosis lsp`. The [editors guide](../docs/EDITORS.md) has
the setup for each one.

| Directory | Editor | |
|---|---|---|
| [`vscode/`](vscode) | VS Code, Cursor, Windsurf, VSCodium | extension: language server, MCP server for Copilot, commands, schemas |
| [`zed/`](zed) | Zed | extension (Rust, WebAssembly): language server and MCP context server |
| [`neovim/`](neovim) | Neovim 0.10+ | plugin: language server, `:Codemosis`, `:CodemosisQuickfix` |
| [`emacs/`](emacs) | Emacs 29+ | lsp-mode add-on, eglot setup, compilation buffer |
| [`helix/`](helix) | Helix | `languages.toml` |
| [`sublime/`](sublime) | Sublime Text | LSP client settings |
