# User guide

Everything you need to use Reverse Codemosis on a project. For writing rules
see the [tutorial](TUTORIAL.md); for CI and editors see
[integrations](INTEGRATIONS.md).

- [Install](#install)
- [The model: distance, findings and what happens to them](#the-model)
- [Understand a project: `map`](#understand-a-project)
- [Find: `check`](#find-check)
- [Decide: questions and answers](#decide-questions-and-answers)
- [Correct: `fix`](#correct-fix)
- [Check the proof: `proof verify`](#check-the-proof)
- [Configure: `codemosis.toml`](#configure)
- [Adopt it on an existing codebase](#adopt-it-on-an-existing-codebase)
- [Files it writes, and which to commit](#files-it-writes)
- [Troubleshooting](#troubleshooting)
- [Command reference](#command-reference)

## Install

```console
$ cargo install --locked --path crates/codemosis-cli      # from a clone
$ cargo install --locked --git https://github.com/graybyrd13/reverse-codemosis codemosis-cli
$ codemosis doctor                                        # check the setup
```

Rust 1.90 or newer is required to build. To correct a project, the tools
that build and test it must be installed (Python, Node, Go, Cargo...), since
every correction is verified with them. `doctor` lists what it found.

## The model

Every finding belongs to one of five **categories**, closed in this order,
most serious first:

| Category | Means | Default level |
|---|---|---|
| broken | code that cannot build, or fails every time it runs | error |
| contradiction | logic that contradicts itself (`x === NaN`, `assert (x, msg)`) | error |
| hazard | an unsafe pattern with a known safe form (bare `except`, fixed waits) | warning |
| waste | dead code and wasted work (unreachable code, unused imports) | note |
| simplification | a structure a simpler one provably replaces | note |

The **distance** of a project is the count of open findings per category.
Each finding has a **status**:

| Status | Means | What happens |
|---|---|---|
| fixable | every fact the rule needs holds; a correction is proven | `fix` applies it and verifies it |
| needs answer | the correction depends on something only you know | it asks; your answer decides |
| explained | no correction can be proven; the explanation is the result | you decide what to do |
| refused | a correction exists, but a guard refused it here (a comment would be deleted, the code is inside `#if`...) | the reason is shown |
| declined | you answered that the code is intentional | not reported again |
| suppressed | you suppressed it, with a reason | not counted; listed with `--show-suppressed` |

Corrections declare what they do to behaviour: **preserved** (identical
behaviour), **corrected** (self-contradictory code gets the only meaning it
can have had) or **repaired** (code that could not run now runs). The pack
self-tests check each claim by running the rule's examples.

## Understand a project

```console
$ codemosis map
```

shows what the engine sees before it changes anything: each language with
its share and tier, how many files are generated or vendored (never
edited), the build and test commands it would use, the definitions it found
and the current distance.

Each file also has a **purpose** from its path: library, test, example,
script (including build files), docs or benchmark. Some rules stay quiet
outside library code, where the behaviour is usually the point: console
output in an example, a pause in a demonstration.

Languages are understood at one of three tiers: **deep** (a syntax tree:
shapes, scopes, facts), **lexical** (it tells code from comments and
strings) and **recognized** (identity only). `codemosis languages` lists all
845, and [LANGUAGES.md](LANGUAGES.md) shows what each tier gets. A file
whose language is only a guess (an ambiguous extension with no evidence) is
listed but never touched.

## Find: `check`

```console
$ codemosis check            # the current directory
$ codemosis check path/to/project --rule python-core   # one pack
```

`check` changes nothing. Each finding looks like a compiler message:

```text
contradiction  javascript-core/nan-comparison  Comparison with NaN
  --> src/cart.js:24:38
     |
  24 |     return this.items.some((item) => item.price === NaN);
     |                                      ^^^^^^^^^^^^^^^^^^
  = `item.price === NaN` is always false: NaN is not equal to anything, itself included.
  = proven correction available (corrected: behaviour changes from self-contradictory to the code's evident meaning)
```

The first line is the category, the rule and its title; then the place, the
message, any evidence, and the status. `codemosis explain RULE` prints the
rule's full explanation, the facts it requires and its proof.

Useful options:

| Option | Does |
|---|---|
| `--diff` | show the proven corrections as a diff, without writing or verifying |
| `--format json` | every finding with its fingerprint, level and confidence (schema `codemosis/check/v1`) |
| `--format sarif\|gitlab\|github\|review\|markdown` | for code scanning, GitLab, GitHub Actions, pull-request suggestions, comments ([integrations](INTEGRATIONS.md)) |
| `--output FILE` | write the report to a file |
| `--fail-on LEVEL` | the exit-code threshold (below) |
| `--show-suppressed` | list suppressed findings with who suppressed them and why |
| `--timings` | where the time went |

Exit codes: **0** when no open finding reaches the `--fail-on` threshold
(default: any open finding), **1** when one does, **2** on an error.

## Decide: questions and answers

Some corrections depend on a fact no analysis can establish: is this fixed
wait deliberate? Which side of this merge conflict should stay? Was this
string meant to be an f-string? The rule asks:

```console
$ codemosis ask                       # one question at a time
$ codemosis answer Q-a7b2ce1d remove  # one answer, from a script
$ codemosis fix --interactive         # ask first, then correct
```

Questions are yes/no ("yes" applies the correction) or offer named options,
each saying what it means. An answer that keeps the code records it as
intentional; it is not asked again. Answers are stored in
`.codemosis/answers.toml` with the question and the place, so a reviewer
sees every human decision a correction depended on. A question's identity
comes from the rule and the code, not the line number, so answers survive
unrelated edits.

## Correct: `fix`

```console
$ codemosis fix
```

`fix` needs a clean git working tree (so `git diff` shows exactly what it
did and `git checkout .` undoes it; `--allow-dirty` and `--allow-no-vcs`
override this). It then:

1. measures the baseline: runs the build and the tests, recording each
   test's result;
2. picks the most serious category with a proven correction, and the
   corrections of one rule that do not overlap;
3. applies them and checks the **acceptance contract**: every target is
   gone, nothing more serious appeared, every file still parses, no name
   changed meaning, every formatted file is still formatted, the build did
   not regress, and no passing test failed;
4. keeps the step, or rolls it back. If a step with several corrections
   fails, it bisects to find the one at fault and reports it with the
   reasons;
5. repeats until nothing provable is left, then prints what changed, what
   was rejected and why, and what still needs you.

The build and test commands come from `codemosis.toml` or are detected
(Cargo, Go, npm/pnpm/yarn/bun scripts, Maven, Gradle, .NET, Mix, SwiftPM,
Bundler, Composer, pytest/unittest, Make). `codemosis init` writes the
detected commands to `codemosis.toml` for you to check.

| Option | Does |
|---|---|
| `--interactive` | ask the open questions first |
| `--dry-run` | plan in memory and show the diff; write nothing, run nothing |
| `--no-verify` | skip the build and tests (structural checks and rule proofs only) |
| `--diff` | print the whole diff at the end |
| `--report FILE` | write a Markdown (or `.json`) report, for example for a pull request |
| `--rule ID` | only these rules or packs |

### Formatting

If your project keeps its code formatted, corrections keep it that way.
`codemosis doctor` and `codemosis map` show which formatter governs which
files and why: rustfmt (`rustfmt.toml` or `Cargo.toml`), gofmt (all Go),
Black or Ruff (`pyproject.toml`, `ruff.toml`, pre-commit hooks), Prettier
(its config or `package.json`), clang-format (`.clang-format`). A file the
formatter would already rewrite is not held to it.

By default a correction that would leave a formatted file unformatted is
refused, with the line the formatter would change:

```text
  ✗     rust-core/manual-assert crates/ignore/src/dir.rs:208  rejected by verification
          crates/ignore/src/dir.rs is kept formatted with rustfmt, and after the correction
          rustfmt would rewrite line 208; set `[format] mode = "reformat"` to let rustfmt lay it out
```

With `[format] mode = "reformat"` the formatter lays out the corrected lines
instead (`✓ 1 rust-core/manual-assert crates/ignore/src/dir.rs:208  laid out
by rustfmt`), as long as every line it changes is within three lines of the
correction. The formatter must be installed; Prettier runs only when the
build and tests run too, because it can execute the project's
configuration.

## Check the proof

Every `fix` that changes code writes `.codemosis/proofs/<date>-<id>.json`.
Anyone can check it:

```console
$ codemosis proof verify .codemosis/proofs/2026-11-04-343c7cc839d8.json
```

Verification recovers the original code from the recorded patch and
replays each step with the pinned rules, checking that every one reproduces
the recorded change exactly. It confirms the diff is what the rules prove
(given the recorded answers); the recorded test results are evidence to
re-run, not something it can replay. A step a formatter laid out is
replayed with the same formatter version when it is installed; otherwise
verification accepts the layout only if it changes whitespace alone.

## Configure

Every section of `codemosis.toml` is optional. A complete example:

```toml
[project]
include = ["src", "tests"]          # only these paths (default: everything)
exclude = ["src/generated"]         # in addition to .gitignore

[oracle]                            # how the project is built and tested
build = "python3 -m compileall -q src"
test = "python3 -m pytest -q"
timeout = 600                       # seconds per command
env = { PYTHONPATH = "src" }

[packs]
builtin = ["python-core", "universal"]   # default: every built-in pack
paths = ["../company-rules"]             # your own packs

[rules]
disable = ["python-core/redundant-pass"]

[policy]
fail_on = "warning"                 # error | warning | note | any | never | a category
baseline = ".codemosis/baseline.json"

[severity]                          # rule id, pack id or category = error | warning | note
"python-core/unused-import" = "warning"
waste = "warning"

[[suppress]]
rule = "python-core/bare-except"    # rule id, pack id or "*"
paths = ["legacy/**"]
reason = "legacy worker, replaced in Q1"
until = "2027-01-31"                # optional: stops applying after this day
owner = "@payments"                 # optional

[format]
mode = "refuse"                     # refuse (default) | reformat | off
disable = ["clang-format"]          # formatters never to run
```

Editors can validate this file with `schemas/config.schema.json`.

## Adopt it on an existing codebase

1. `codemosis check` to see where the project stands.
2. `codemosis baseline` to accept today's findings, then
   `codemosis check --baseline .codemosis/baseline.json` (or
   `[policy] baseline`) so only **new** findings fail. Commit the baseline;
   `check` tells you when entries have been fixed so you can shrink it.
3. Suppress what is intentional, with a reason, next to the code:

   ```python
   except Exception:  # codemosis: ignore[python-core/bare-except] the worker must never die; errors are logged
   ```

   `codemosis: ignore[...]` on its own line covers the next line;
   `ignore-file[...]` covers the file; `until=YYYY-MM-DD` and `owner=` are
   optional.
   Suppressions you already wrote for other tools count too: `# noqa:
   F821`, `# type: ignore[name-defined]`, `// eslint-disable-next-line
   no-console`, `NOLINT`, `//nolint`, `# shellcheck disable=SC2086` and
   `#[allow(clippy::needless_return)]` silence the matching rules, which
   declare their equivalents in other tools (`codemosis explain RULE`).
4. Set the bar with `[policy] fail_on` and `[severity]`.
5. `codemosis lock` to pin the packs, so a change to any rule is reviewed.
6. Add the [GitHub Action, GitLab component or pre-commit hook](INTEGRATIONS.md).

## Files it writes

| File | Contains | Commit it? |
|---|---|---|
| `codemosis.toml` | your configuration | yes |
| `codemosis.lock` | the packs, pinned by content hash | yes |
| `.codemosis/answers.toml` | your answers to questions | yes: they are decisions |
| `.codemosis/baseline.json` | accepted findings | yes, if you use one |
| `.codemosis/proofs/*.json` | proof certificates | yes, with the change they prove |
| `.codemosis/log.jsonl` | the audit log of fixes, answers, baselines, locks | your choice |

The engine never writes outside the project, never through a symbolic
link, and never touches generated or vendored files.

## Troubleshooting

| Symptom | Do |
|---|---|
| "the working tree has uncommitted changes" | commit or stash first, or pass `--allow-dirty` |
| "the packs do not match codemosis.lock" | a pack changed: review it, then `codemosis lock` |
| "'x' is not a rule or pack id" | a name in `[severity]` or `[[suppress]]` is wrong; `codemosis rules` lists them |
| no build or test command found | set `[oracle]` (`codemosis init` writes a starting point) |
| a correction you expected was not made | `codemosis explain RULE` lists its facts; `--log debug` shows why a step was refused |
| a file is ignored | `codemosis map` shows protected files and guessed languages |
| it is slow | `codemosis check --timings` |
| anything else | `codemosis doctor`, then an issue with its output |

## Command reference

| Command | What it does |
|---|---|
| `check [PATH]` | report every finding; changes nothing (`--file F`, `--changed`, `--format compact`) |
| `fix [PATH]` | apply every provable correction, verifying each step |
| `ask [PATH]` | ask the open questions one at a time |
| `answer ID ANSWER` | record one answer |
| `map [PATH]` | what the engine understands about the project |
| `languages` | every language GitHub knows, with its tier (`--format json\|markdown`) |
| `explain RULE` | a rule's explanation, facts, question and proof |
| `rules` | every rule of every pack |
| `baseline [PATH]` | accept today's findings |
| `lock [PATH]` | pin the packs (`--check` verifies only) |
| `proof verify CERT` | replay a proof certificate |
| `doctor [PATH]` | check configuration, packs, oracle and toolchains |
| `init [PATH]` | write a `codemosis.toml` with the detected commands |
| `lsp` | the language server, for editors ([editors](EDITORS.md)) |
| `mcp` | the MCP server, for agentic coding tools ([agent tools](AGENT_TOOLS.md)) |
| `hook` | for agents' hooks: check what the agent just changed |
| `integrate [TOOL]` | connect an agent or editor: Claude Code, Cursor, VS Code, Gemini CLI, Zed, Aider... |
| `pack test\|lint\|docs\|new` | work with packs ([tutorial](TUTORIAL.md)) |
| `tree FILE` | print a syntax tree, to write rule shapes |
| `facts` | the fact vocabulary for rules |
| `schema [NAME]` | JSON Schemas of the file formats |
| `completions SHELL`, `man` | shell completions and man pages |

Every command takes `--color auto|always|never` and `--log debug|trace`;
`codemosis help COMMAND` shows every option.
