# Integrations

## Continuous integration

### GitHub Actions

```yaml
permissions:
  contents: read
  security-events: write   # only for sarif: true
  pull-requests: write     # only for review: true

jobs:
  codemosis:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: graybyrd13/reverse-codemosis@main
        with:
          fail-on: warning        # error | warning | note | any | never | a category
          baseline: .codemosis/baseline.json   # optional: fail only on new findings
          sarif: true             # upload to GitHub code scanning
          review: true            # suggested changes for proven corrections on PRs
```

The action annotates findings on the diff, writes a summary to the job page,
and fails according to `fail-on`. With `sarif: true` findings appear in the
repository's Security tab, with rule help, CWE tags and suppressions; with
`review: true` every proven correction on a changed file becomes a
suggested change a reviewer can commit with one click.

Without the action: `codemosis check --format github` (annotations and job
summary), `--format sarif --output codemosis.sarif`, `--format review`
(a review body for the GitHub API).

### GitLab

Include the CI/CD component in `templates/codemosis.yml`:

```yaml
include:
  - component: $CI_SERVER_FQDN/<group>/reverse-codemosis/codemosis@<version>
    inputs:
      fail-on: warning
```

Findings appear in the merge request's Code Quality widget
(`codemosis check --format gitlab`), and a SARIF report is kept as an
artifact.

### pre-commit

```yaml
repos:
  - repo: https://github.com/graybyrd13/reverse-codemosis
    rev: <tag or commit>
    hooks:
      - id: codemosis-check          # fails on warnings and errors
      # - id: codemosis-check-strict # fails on any finding
      # - id: codemosis-pack-test    # for repositories of packs
```

### Gating and adoption

| Need | Use |
|---|---|
| Fail only on serious findings | `--fail-on error`, or `[policy] fail_on = "error"` |
| Adopt on an existing codebase | `codemosis baseline`, then `check --baseline .codemosis/baseline.json` |
| Accept one finding, with a reason | `# codemosis: ignore[rule-id] reason until=2027-01-31` next to it |
| Accept a rule in a directory | `[[suppress]]` with `paths` and a `reason` in `codemosis.toml` |
| Make a rule louder or quieter | `[severity] "rule-id" = "error"` |
| Reproducible packs | `codemosis lock`, committed; CI fails if a pack changes |

Every format carries a stable fingerprint per finding (independent of line
numbers), so hosts track findings across commits.

## Editors

`codemosis lsp` is a Language Server: diagnostics as you type, hover for the
rule's explanation, proven corrections as quick fixes, "fix all" as a source
action, and questions answered from the editor (recorded in
`.codemosis/answers.toml` exactly as `codemosis answer` would). It never runs
your build or tests and never writes files itself; `codemosis fix` remains
the command that verifies corrections.

**Neovim** (0.10+):

```lua
vim.api.nvim_create_autocmd("FileType", {
  callback = function(args)
    vim.lsp.start({
      name = "codemosis",
      cmd = { "codemosis", "lsp" },
      root_dir = vim.fs.root(args.buf, { "codemosis.toml", ".git" }),
    })
  end,
})
```

**Helix** (`languages.toml`):

```toml
[language-server.codemosis]
command = "codemosis"
args = ["lsp"]

[[language]]
name = "python"
language-servers = ["pyright", "codemosis"]
```

**Emacs** (eglot):

```elisp
(add-to-list 'eglot-server-programs '((python-mode js-mode rust-mode) . ("codemosis" "lsp")))
```

**VS Code**: any generic LSP client extension can run `codemosis lsp`; point
its server command at the `codemosis` binary.

## Editing configuration and packs

`schemas/` holds JSON Schemas for `codemosis.toml`, `pack.toml`, rule files,
`codemosis.lock`, baselines and proof certificates (`codemosis schema`
prints them). With Taplo or Even Better TOML, add a first line such as

```toml
#:schema https://raw.githubusercontent.com/graybyrd13/reverse-codemosis/main/schemas/rule.schema.json
```

for completion and validation as you type.

## Shells

```console
$ codemosis completions bash > ~/.local/share/bash-completion/completions/codemosis
$ codemosis completions zsh  > "${fpath[1]}/_codemosis"
$ codemosis completions fish > ~/.config/fish/completions/codemosis.fish
$ codemosis man --output ~/.local/share/man/man1
```

## Diagnosing problems

```console
$ codemosis doctor          # configuration, packs, lockfile, oracle, toolchains
$ codemosis check --timings # where the time goes
$ codemosis --log debug fix # every step, and why it was kept or rolled back
```
