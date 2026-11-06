# Reverse Codemosis

**Correct code the way a proof corrects an argument.**

Reverse Codemosis is a command-line tool, written in Rust with no AI model
inside, that:

1. **finds** every place a project's code falls short of its best form;
2. **corrects** what it can prove, and only that, checking every step
   against the project's own build and tests;
3. **explains** what it cannot correct, in plain language, with the evidence;
4. **asks** when the decision is genuinely yours, and records your answer.

It never guesses. When it is not sure, it does nothing and says why.

| | When the code is wrong it... | You get |
|---|---|---|
| A linter | tells you, and leaves the change to you | a list of warnings |
| An AI assistant | rewrites it, whether or not the rewrite is right | plausible new code to check |
| **Reverse Codemosis** | changes it only when a rule proves the change safe, and rolls back anything your tests disagree with | a diff, the reasons, and a proof anyone can replay |

## See it work

[`examples/broken-inventory`](examples/broken-inventory) is a small Python
project that does not build and fails every test. One command:

```console
$ codemosis fix
```

```text
Baseline
  build     failed (1 error)
  tests     0 passed · 4 failed

Corrections
  ✓  1  python-core/missing-module-import 2 corrections in inventory/report.py  build and tests unchanged
  ✓  2  python-core/missing-name-import inventory/store.py:27  tests 0 → 10 passing
  ✓  3  python-legacy/except-comma inventory/pricing.py:16  tests 10 → 18 passing
  ...
  ✓ 11  python-core/return-bool-if inventory/store.py:29  build and tests unchanged

Result
  build     ok
  tests     21 passed · 0 failed
  applied   14 corrections in 4 files; review them with `git diff`

Needs your decision (3)
...
Proof certificate: .codemosis/proofs/2026-11-04-343c7cc839d8.json
```

The result is [a diff](examples/broken-inventory.diff) you can read and
approve line by line, three questions only you can answer, and a
certificate that `codemosis proof verify` replays to show every change is
exactly what the rules prove. A JavaScript demonstration,
[`examples/flaky-checkout`](examples/flaky-checkout), does the same for a
flaky test suite, asking before it removes each fixed wait.

## Quick start

```console
$ cargo install --locked --path crates/codemosis-cli   # installs `codemosis`
$ cd your-project                  # a git repository with a clean working tree
$ codemosis map                    # what it understands about the project
$ codemosis check                  # every finding; changes nothing
$ codemosis fix --interactive      # answer the questions, then apply what can be proven
$ git diff                         # read what changed
```

`codemosis doctor` checks your setup. The [user guide](docs/GUIDE.md) walks
through everything else.

## How it decides

- **Rules come from fix packs**: plain TOML files anyone can read and write.
  A rule says *when this shape appears and these facts hold, this
  correction is safe*, and carries the examples that prove it.
- **Every rule proves itself** before it ships: `codemosis pack test`
  applies it to its examples and runs the code before and after with the
  real interpreter or compiler, checking that behaviour is preserved (or
  corrected, or repaired) exactly as the rule claims.
- **Facts are conservative**: when one cannot be sure, it blocks the
  correction. A missed fix is acceptable; a wrong fix is not.
- **Your tests have the last word**: each step is kept only if the build
  does not regress and no passing test fails; otherwise it is rolled back
  and reported with its evidence.
- **Your formatter's too**: a file kept formatted by rustfmt, gofmt, Black,
  Ruff, Prettier or clang-format stays formatted. A correction that would
  break that is refused, or (if you allow it) laid out by the formatter,
  next to the correction only.
- **Some code is never touched**: comments, generated and vendored files,
  files whose language is a guess, code inside `#if` blocks or behind Rust
  `#[cfg]`, macro bodies, code the grammar could not parse, and (in Python)
  any edit that would change what a name refers to elsewhere.
- **What only you know, it asks**: keep this wait or remove it? Which side
  of this merge conflict? Answers live in `.codemosis/answers.toml`,
  reviewed and committed like code.

The precise model is in [docs/SEMANTICS.md](docs/SEMANTICS.md).

## What it covers today

- **Languages**: all 845 languages GitHub knows. 30 are understood deeply
  (Python, JavaScript, TypeScript, TSX, Go, Java, C, C++, C#, Rust, Ruby,
  PHP, Shell, Kotlin, Scala, Lua, Swift, Elixir, Erlang, Haskell, OCaml,
  Zig, Dart, R, Julia, Elm, Nix, HCL/Terraform, PowerShell, Solidity), 330
  lexically (comments and strings), the rest by identity.
  [Details](docs/LANGUAGES.md).
- **Rules**: 27 built-in packs with 77 rules: missing imports, identity
  comparisons with literals, `=== NaN`, unreachable code, unused imports,
  vacuous asserts, empty catches, debugging leftovers, fixed waits in 48
  languages, merge conflicts, committed keys, public buckets and secrets in
  Terraform, `tx.origin` in Solidity, unpinned Nix fetches, and more.
  [Reference](docs/packs/README.md).
- **Agents**: an MCP server (`codemosis mcp`) and a hook (`codemosis hook`)
  for Claude Code, Cursor, Copilot, Codex, Gemini CLI, Zed, Aider and more.
  Agents get the proven corrections, and bring the questions back to you.
  `codemosis integrate <tool>` sets it up. [Agent tools](docs/AGENT_TOOLS.md).
- **Editors**: a language server with extensions for VS Code and Zed, and
  setups for Neovim, Helix, Emacs, Sublime Text and JetBrains.
  [Editors](docs/EDITORS.md).
- **Workflows**: SARIF for code scanning, GitLab Code Quality, GitHub
  annotations and suggested changes, a GitHub Action, pre-commit hooks,
  baselines and suppressions for adopting it on an existing codebase.
  [Integrations](docs/INTEGRATIONS.md).

The rules are deliberately narrow: each corrects one well-understood
mistake, and only where it can prove the correction. On twenty well-known
open-source projects (10,780 files, eleven languages, 15 seconds in all) it
proposed 127 corrections, every one of which was reviewed and is correct,
asked 128 questions a careful reviewer would also ask, found three genuine
bugs, and found nothing at all in nine projects. It honours the
suppressions those projects already wrote for flake8, mypy, eslint and
other tools. [docs/FIELD_TEST.md](docs/FIELD_TEST.md) has the details,
including what it got wrong before the test.

## What it refuses to do

It does not design. It will not invent features, choose architectures or
write a program from a wish. It takes code that exists, including the flood
of almost-right code now coming out of AI tools, and returns code that is
correct and clean as far as the rules can prove. Design is a later chapter
that begins only once correction is trusted completely.

## Documentation

| I want to... | Read |
|---|---|
| use it on my project | [User guide](docs/GUIDE.md) |
| run it in CI | [Integrations](docs/INTEGRATIONS.md) |
| use it with Claude Code, Cursor, Copilot or another agent | [Agent tools](docs/AGENT_TOOLS.md) |
| use it in my editor | [Editors](docs/EDITORS.md) |
| write a rule | [Tutorial](docs/TUTORIAL.md), then [pack format](docs/PACK_FORMAT.md) and [facts](docs/FACTS.md) |
| work on the engine | [Developer guide](docs/DEVELOPING.md), [architecture](docs/ARCHITECTURE.md) |
| know exactly what "proven" means | [Semantics](docs/SEMANTICS.md) |
| see what comes next | [Roadmap](docs/ROADMAP.md) |

Everything is indexed in [docs/README.md](docs/README.md).

## Contributing

Rules are the most valuable contribution: every rule a stranger writes makes
the tool stronger for everyone. Start with the [tutorial](docs/TUTORIAL.md)
and [CONTRIBUTING.md](CONTRIBUTING.md). Report a correction that made code
worse with the "A correction made code worse" issue template; it is treated
as the most serious kind of bug. Security reports go through
[SECURITY.md](SECURITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Contributions are accepted under
the same terms. The language catalogue in [`data/linguist`](data/linguist)
is GitHub Linguist's, used under its MIT license.
