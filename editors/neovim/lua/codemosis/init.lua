-- Reverse Codemosis for Neovim.
--
--   require("codemosis").setup()
--
-- Starts `codemosis lsp` (findings, proven corrections as code actions,
-- questions answered with code actions, hover), and adds:
--
--   :Codemosis check | changed | fix | preview | ask | map   (in a terminal)
--   :CodemosisQuickfix [changed]                              (findings in the quickfix list)

local M = {}

M.options = {
  cmd = "codemosis",
  lsp = true,
}

local function root()
  return vim.fs.root(0, { "codemosis.toml", ".git" }) or vim.fn.getcwd()
end

local function terminal(args)
  vim.cmd("botright split")
  vim.fn.termopen(vim.list_extend({ M.options.cmd }, args), { cwd = root() })
  vim.cmd("startinsert")
end

local subcommands = {
  check = { "check" },
  changed = { "check", "--changed" },
  preview = { "fix", "--dry-run" },
  fix = { "fix", "--interactive" },
  ask = { "ask" },
  map = { "map" },
}

-- `path:line:col: level: message [rule; status]`
local errorformat = table.concat({
  "%f:%l:%c: %trror: %m",
  "%f:%l:%c: %tarning: %m",
  "%f:%l:%c: %tote: %m",
}, ",")

function M.quickfix(changed)
  local args = { M.options.cmd, "check", "--format", "compact", "--fail-on", "never" }
  if changed then
    table.insert(args, "--changed")
  end
  local result = vim.system(args, { cwd = root(), text = true }):wait()
  local lines = vim.split(result.stdout or "", "\n", { trimempty = true })
  vim.fn.setqflist({}, " ", { title = "Reverse Codemosis", lines = lines, efm = errorformat })
  if #lines == 0 then
    vim.notify("Reverse Codemosis: nothing found", vim.log.levels.INFO)
  else
    vim.cmd("copen")
  end
end

local function start_lsp()
  if vim.lsp.enable then
    -- Neovim 0.11+.
    vim.lsp.config("codemosis", { cmd = { M.options.cmd, "lsp" }, root_markers = { "codemosis.toml", ".git" } })
    vim.lsp.enable("codemosis")
    return
  end
  vim.api.nvim_create_autocmd("FileType", {
    group = vim.api.nvim_create_augroup("codemosis", { clear = true }),
    callback = function(ev)
      if vim.bo[ev.buf].buftype ~= "" then
        return
      end
      vim.lsp.start({
        name = "codemosis",
        cmd = { M.options.cmd, "lsp" },
        root_dir = vim.fs.root(ev.buf, { "codemosis.toml", ".git" }),
      })
    end,
  })
end

function M.setup(opts)
  M.options = vim.tbl_deep_extend("force", M.options, opts or {})
  if M.options.lsp then
    start_lsp()
  end
  vim.api.nvim_create_user_command("Codemosis", function(o)
    local args = subcommands[o.args ~= "" and o.args or "check"]
    if not args then
      vim.notify("Codemosis: unknown subcommand " .. o.args, vim.log.levels.ERROR)
      return
    end
    terminal(args)
  end, {
    nargs = "?",
    complete = function()
      return vim.tbl_keys(subcommands)
    end,
    desc = "Run Reverse Codemosis in a terminal",
  })
  vim.api.nvim_create_user_command("CodemosisQuickfix", function(o)
    M.quickfix(o.args == "changed")
  end, {
    nargs = "?",
    complete = function()
      return { "changed" }
    end,
    desc = "Reverse Codemosis findings in the quickfix list",
  })
end

return M
