-- Neovim 0.11+: `vim.lsp.enable("codemosis")` reads this file.
-- No `filetypes`: Reverse Codemosis reads every language, so it attaches to all.
---@type vim.lsp.Config
return {
  cmd = { "codemosis", "lsp" },
  root_markers = { "codemosis.toml", ".git" },
}
