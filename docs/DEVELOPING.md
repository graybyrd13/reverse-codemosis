# Developer guide

How the repository is laid out, how a command flows through the code, and
step-by-step recipes for the changes you will make most often. For the
design rationale see [ARCHITECTURE.md](ARCHITECTURE.md); for the exact
guarantees see [SEMANTICS.md](SEMANTICS.md).

## The repository

```text
reverse-codemosis/
├── crates/
│   ├── codemosis-core/        finding problems; changes nothing on disk
│   │   ├── src/model.rs         findings, categories, statuses, edits
│   │   ├── src/linguist.rs      the 845 languages and file detection
│   │   ├── src/lex.rs           code / comment / string for 343 languages
│   │   ├── src/project.rs       every file of a project, parsed once
│   │   ├── src/lang/            the 17 grammars: python.rs (+ python_scope.rs),
│   │   │                        rust.rs, generic.rs (a table of 15 more)
│   │   ├── src/pack.rs          reading and compiling packs
│   │   ├── src/rule.rs          matching a rule, building its edits, scan()
│   │   ├── src/expr.rs, eval.rs the fact language and its vocabulary
│   │   ├── src/template.rs      ${capture|filter} templates
│   │   ├── src/guards.rs        regions no edit may touch
│   │   ├── src/suppress.rs      inline `codemosis: ignore[...]`
│   │   ├── src/redact.rs        secrets out of every output
│   │   ├── src/selftest.rs      pack proofs and mutation testing
│   │   ├── src/lint.rs          pack lint
│   │   └── tests/               conformance (+ corpus/), properties, security, facts
│   ├── codemosis-engine/      changing code, verified
│   │   ├── src/lib.rs           Workspace: config + packs + answers + project
│   │   ├── src/session.rs       the correction loop and its acceptance contract
│   │   ├── src/oracle.rs        running the build and tests; testparse.rs reads results
│   │   ├── src/formatter.rs     the project's formatters: detection, checking, layout
│   │   ├── src/policy.rs        severities, suppressions, the gate, baselines
│   │   ├── src/ci.rs            SARIF, GitLab, GitHub annotations, reviews
│   │   ├── src/proof.rs         certificates; patch.rs exact diffs; lock.rs; audit.rs
│   │   ├── src/fsguard.rs       every write goes through here
│   │   ├── tests/soundness.rs   adversarial cases that must be refused
│   │   └── tests/formatting.rs  the formatting contract end to end (needs gofmt)
│   └── codemosis-cli/         the `codemosis` command
│       ├── src/main.rs          commands and their options
│       ├── src/lsp.rs           the language server, for editors
│       ├── src/mcp.rs           the MCP server, for agentic coding tools
│       ├── src/agent.rs         scoped checks, compact output, `codemosis hook`
│       ├── src/integrate.rs     `codemosis integrate <tool>`
│       ├── src/doctor.rs, map.rs, interactive.rs, packdocs.rs, scaffold.rs
│       └── tests/               demo (goldens), policy, proof, lsp, mcp, agent, docs_links
├── packs/                     built-in packs, embedded into the binary at build time
├── data/linguist/             Linguist's languages.yml, read by build.rs
├── examples/                  the two demonstrations and their golden diffs
├── docs/                      this documentation; FACTS.md, LANGUAGES.md, packs/ are generated
├── schemas/                   generated JSON Schemas of the file formats
├── scripts/                   demo.sh; field-test.sh (+ pins, baseline, summary); field-test-oracle.sh
├── editors/                   VS Code and Zed extensions, Neovim, Emacs, Helix, Sublime
├── integrations/              agentic coding tools: the Claude Code plugin (.claude-plugin/ lists it)
├── action.yml, templates/, .pre-commit-hooks.yaml   CI integrations
└── rfcs/, GOVERNANCE.md, SECURITY.md, CHANGELOG.md
```

`codemosis-core` must not write files or run project commands (it runs
interpreters only for pack self-tests). `codemosis-engine` changes code and
runs the oracle. `codemosis-cli` is presentation and wiring.

## Setting up

```console
$ cargo build
$ cargo run -q -- doctor     # which toolchains are present
```

Tests execute pack examples, so `python3`, `rustc` and `node` (22.6+) must
be on the PATH. Other runners (`go`, `javac`, `cc`, `c++`, `ruby`, `php`,
`bash`, `lua`, `swift`) are used when present; a missing one turns its
checks into reported skips, never passes.

## The loop

```console
$ cargo fmt --all
$ cargo clippy --all-targets -- -D warnings
$ cargo test --workspace                       # everything below, about a minute
$ cargo run -q -- pack test                    # pack proofs, in detail
$ cargo run -q -- pack lint --deny-warnings
$ cargo run -q -- check crates                 # the tool on its own source: must stay clean
```

Built-in packs are compiled into the binary, so **rebuild after editing a
pack** before running `pack test` through `target/debug/codemosis`
(`cargo run` does this for you).

| Test | Where | Guards |
|---|---|---|
| unit tests | next to the code (`#[cfg(test)]`) | each module |
| pack self-tests | `cargo run -- pack test` (also in `demo.rs`) | every rule's claims |
| soundness | `crates/codemosis-engine/tests/soundness.rs` | the guards: unsound rules must be refused |
| conformance | `crates/codemosis-core/tests/conformance.rs` + `corpus/` | every grammar: comments, strings, definitions, errors |
| properties | `crates/codemosis-core/tests/properties.rs` | lexing, detection, parsers, redaction, edits (`CODEMOSIS_PROPTEST_CASES`) |
| security | `crates/codemosis-core/tests/security.rs`, `fsguard`, `process` | the threat model |
| policy, proof, LSP | `crates/codemosis-cli/tests/*.rs` | end to end through the binary |
| demonstrations | `crates/codemosis-cli/tests/demo.rs` | golden diffs and generated docs |

### Generated files

These are checked by tests and regenerated with one command:

```console
$ UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo
```

| File | Produced by |
|---|---|
| `examples/*.diff` | the demonstrations (review every line before accepting) |
| `docs/FACTS.md` | `codemosis facts` |
| `docs/LANGUAGES.md` | `codemosis languages --format markdown` |
| `docs/packs/*.md` | `codemosis pack docs` |
| `schemas/*.json` | `codemosis schema --output schemas` |

## How a command flows

**`check`**: `main.rs::check` → `Workspace::open` (load `codemosis.toml`,
read and parse every file in parallel, compile only the packs for the
grammars present, verify the lockfile and policy) → `Workspace::scan_rules`
→ `codemosis_core::scan` (every rule on every file, in parallel; supersede
moot findings; apply inline suppressions) → `Policy::apply_suppressions` →
format (`report.rs` for text, `ci.rs` for the CI formats) → exit code from
`Policy::fails`.

**`fix`**: as above, then `Session::run`: baseline oracle run → loop {
choose the most serious rule with fixable findings → `try_step` (apply
edits in memory → syntax check → binding check → rescan the touched files
→ write → oracle → keep, or restore and bisect) } → report →
`proof::Certificate::of` → `audit::record`.

**A rule match**: `CompiledRule::find` runs the tree-sitter query (or the
regular expression for text rules), evaluates `let` and `where` for each
match, renders the summary and question, builds edits through the guards
(`guards::unverifiable`, comment preservation, line integrity), and
anchors the finding to code rather than line numbers.

## Recipes

### Add a rule to a built-in pack

1. Write the bad code in a file and run `cargo run -q -- tree file.ext`.
2. Create `packs/<pack>/rules/<id>.toml` (the file name must be the id).
   Start from a similar rule or `codemosis pack new`. The
   [tutorial](TUTORIAL.md) walks through it.
3. Examples that would catch a wrong rule; counterexamples for every case a
   careless rule would get wrong.
4. `cargo run -q -- pack test --rule <id> -v`, then `pack lint` and
   `pack test --mutations --rule <id>`: every fact should be needed by a
   counterexample.
5. `UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo` to refresh
   `docs/packs/`.
6. Try it on real code (`docs/FIELD_TEST.md` lists repositories that make
   good testbeds).

### Add a fact

1. Generic facts live in `eval.rs` (`GENERIC_FACTS` and the `call` match);
   language facts in `lang/<language>.rs` (`facts()` and `call()`).
2. Write its `FactDoc`: the exact meaning, and in which direction it errs
   when unsure. It must err towards blocking the correction.
3. Test it in `crates/codemosis-core/tests/` (see `python_facts.rs`),
   including the cases where it must answer "not sure".
4. Use it in a rule whose counterexamples need it.
5. Regenerate `docs/FACTS.md`.

### Teach the engine a language

- **A table entry** (a wait idiom, say): one line in
  `packs/waits/tables/waits.toml`, keyed by the GitHub language name, plus
  an example in `packs/waits/rules/fixed-wait.toml`. The pattern must match
  the whole statement on its line and capture it as `call`.
- **Lexical tier**: map the language to a profile in `core/src/lex.rs`,
  with a test telling a comment, a string and code apart.
- **Deep tier**: add the tree-sitter crate to `Cargo.toml` and
  `crates/codemosis-core/Cargo.toml`, a row in `lang/generic.rs`
  (grammar, Linguist names, example file name, definition and declaration
  node kinds, a `CommandRunner`), a mapping in `lang::grammar_crate`, a
  corpus in `crates/codemosis-core/tests/corpus/<id>/`, and a
  `<language>-core` pack. The conformance suite tells you what is missing.
- **The language list** itself comes from Linguist; see
  [data/linguist/README.md](../data/linguist/README.md) to update it.

### Add a guard

Guards refuse edits whatever the rule says. Put region checks in
`core/src/guards.rs::unverifiable` and whole-step checks in
`engine/src/session.rs::try_step`. Every guard needs a case in
`soundness.rs` that fails without it, and a line in
[SEMANTICS.md](SEMANTICS.md) and the threat model.

### Add a command or option

Commands are the `Command` enum in `main.rs`; put substantial logic in its
own module. Doc comments become `--help`, man pages and completions. Write
to project files only through `fsguard`, and print source text only after
`redact`. Add an end-to-end test in `crates/codemosis-cli/tests/`.

### Add an output format

Add a function to `engine/src/ci.rs` taking `CheckOutput`, a variant to
`CheckFormat` in `main.rs`, and a test. Carry the fingerprint and redact
excerpts.

### Change a file format or schema

Formats are interfaces ([COMPATIBILITY.md](COMPATIBILITY.md)). Bump the
schema id, keep reading the previous version, regenerate `schemas/`, note
it under "Breaking" in the changelog, and open an RFC for anything beyond a
backward-compatible addition.

## Principles the code follows

- **Never guess.** When a fact, detector or guard is unsure, it blocks.
  Prefer a missed correction to a wrong one, and say why in the output.
- **Deterministic output.** Same input, same output, whatever the thread
  count. Sort before printing; `parallel::map` keeps input order.
- **Every write is confined** (`fsguard`) and **every printed excerpt is
  redacted** (`redact`).
- **No network.** The engine makes no requests and collects no telemetry.
- **Errors explain what to do next**, the way the lockfile and dirty-tree
  errors do.

## Debugging

| Question | Command |
|---|---|
| What is the engine doing? | `--log debug` (or `trace`; `CODEMOSIS_LOG_FORMAT=json`) |
| Why was a step rolled back? | `codemosis fix --log debug`, or the "Rejected" section of the report |
| What does the tree look like? | `codemosis tree file.ext` |
| Which facts exist? | `codemosis facts --language python` |
| Where does the time go? | `codemosis check --timings`; `CODEMOSIS_THREADS=1` for a sequential run |
| Is a fact needed? | `codemosis pack test --mutations --rule <id>` |

## Releasing

See [RELEASING.md](RELEASING.md).
