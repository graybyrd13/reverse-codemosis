# Architecture

```text
 ┌──────────────────────────────── codemosis-cli ─────────────────────────────────┐
 │ check · fix · ask · answer · explain · map · languages · rules · baseline      │
 │ lock · proof verify · doctor · lsp · pack test/lint/docs/new · schema · tree   │
 │ facts · init · completions · man          (lsp.rs, doctor.rs, packdocs.rs ...) │
 └───────────────┬──────────────────────────────────────────────┬─────────────────┘
                 │                                              │
 ┌──────── codemosis-engine ───────────┐   ┌────────── codemosis-core ─────────────┐
 │ Workspace  config, packs, policy    │   │ model     findings, status, distance  │
 │ Session    the correction loop      │──▶│ pack      manifest, rules, digests    │
 │ Oracle     build + test commands    │   │ rule      compile, match, edit, scan  │
 │ formatter  rustfmt, gofmt, Black... │   │                                       │
 │ testparse  per-test results         │   │ expr/eval the fact language           │
 │ policy     suppress, severity, gate │   │ template  rewrite templates           │
 │ ci         SARIF, GitLab, GitHub    │   │ lang      17 grammars, bindings       │
 │ proof      certificates + replay    │   │ linguist  845 languages, detection    │
 │ patch      exact unified diffs      │   │ lex       code/comment/string         │
 │ lock       pinned packs             │   │ guards    unverifiable regions        │
 │ audit      decision log             │   │ suppress  inline ignores, dates       │
 │ fsguard    confined writes          │   │ redact    secrets out of output       │
 │ answers    the human-facts ledger   │   │ selftest  proofs, mutation testing    │
 │ report     text, markdown           │   │ lint      pack weaknesses             │
 │ vcs        clean-tree check         │   │ parallel  ordered multi-core map      │
 └─────────────────────────────────────┘   │ project   every file, tier, role      │
                                           │ version   engine ranges · log         │
                                           └───────────────────────────────────────┘
                  packs/  (embedded at build time by build.rs, with data/linguist)
```

## codemosis-core

Everything needed to find problems and describe corrections, without
changing anything on disk.

- **`model`**: `Category` (the five kinds of distance, in priority order),
  `Semantics`, `Finding`, `Status` (fixable, needs answer, declined,
  explained, refused), `Edit`, `Question`, `Distance`.
- **`linguist`**: every language GitHub knows, generated at build time from
  Linguist's `languages.yml` (in `data/linguist/`). Detection follows
  Linguist's order: exact filename, shebang interpreter, longest extension,
  then content heuristics for ambiguous extensions (`.h`, `.m`, `.pl`,
  `.ts`, `.tsx`, `.php`...). Every detection carries a confidence
  (`Certain`, `Heuristic`, `Guess`) and a role (`Source`, `Generated`,
  `Vendored`, `Documentation`); rules never run on a guess and never edit
  generated or vendored files.
- **`lex`**: comment and string syntax for 343 languages, grouped into
  families keyed by Linguist's `ace_mode`, `codemirror_mode` and `tm_scope`.
  `segments()` splits a file into code, comment and string; text rules use
  it to know where a match starts.
- **`project`**: a `Project` is every file of a directory (honouring
  `.gitignore`) with its language, tier and role. Files with a grammar are
  parsed once with tree-sitter and cache one language-specific analysis (for
  Python, the scope analysis), so facts are cheap and can look across files.
  Each file sits in one of three tiers:

  | Tier | Languages | What the engine understands |
  |---|---|---|
  | deep | 17 | the syntax tree: shapes, scopes, definitions, every fact |
  | lexical | 343 | where code, comments and strings are; text rules |
  | recognized | 485 | the language and role; language-independent text rules |
- **`pack`**: reading and validating packs. Built-in packs in `/packs` are
  embedded into the binary by `build.rs`. Compilation collects every problem
  in a pack instead of stopping at the first.
- **`rule`**: `CompiledRule::compile` checks a rule completely before it can
  run: the shape compiles for the language, every capture and fact a rule
  mentions exists, templates and edits are well formed, and examples and
  counterexamples are present. `CompiledRule::find` runs the query, evaluates
  `let` and `where` for each match, renders the summary and question, builds
  the edits (with guards for comments and line integrity), and anchors each
  finding to code rather than to line numbers.
- **`expr` / `eval`**: the fact language and its vocabulary. Generic facts
  live in `eval`; language facts behind `Language::call`.
- **`template`**: `${capture|filter}` templates.
- **`lang`**: the `Language` trait (grammar, facts, filters, import
  placement, a snippet runner). Python (`python.rs`, `python_scope.rs`) and
  Rust (`rust.rs`) have hand-written scope facts; `generic.rs` is a table of
  fifteen more grammars (JavaScript, TypeScript, TSX, Go, Java, C, C++, C#,
  Ruby, PHP, Bash, Kotlin, Scala, Lua, Swift), each with its comment kinds,
  definition kinds, import convention and a `CommandRunner` for its
  toolchain.
- **`selftest`**: the pack proofs: rewrite to a fixpoint, compare with
  `after`, check idempotence and binding preservation, run before and after
  with the language runner to check the semantics claim, and check
  counterexamples. `mutants()` removes each fact in turn for mutation
  testing.
- **`guards`**: regions no edit may touch: conditional compilation, Rust
  `#[cfg]` items and macro bodies, constructs with parse errors.
- **`suppress`**: inline `codemosis: ignore[...]` markers, with reasons,
  owners and expiry dates, applied in every scan.
- **`redact`**: masks credentials in every message, excerpt and report diff.
- **`lint`**: what makes a rule weak although it loads.
- **`parallel`**: a dependency-free work queue; files are read, parsed and
  matched on every core and results keep input order.
- **`version`**: engine version requirements for packs. **`log`**: the
  `--log debug|trace` diagnostics.

## codemosis-engine

- **`Workspace`** opens a project: `codemosis.toml`, the packs that apply,
  the answers ledger and the parsed source.
- **`Oracle`** runs the project's build and test commands (in the project
  directory, with a time limit, keeping Python bytecode out of the tree) and
  reads per-test results with `testparse` (unittest, pytest, libtest, Go,
  TAP, Jest/Vitest/Mocha/node:test spec output, dotnet). `config` detects
  the oracle from the project: Cargo, Go modules, npm scripts, Maven,
  Gradle, dotnet, Bundler/Rake, Composer, CMake, Make, pytest or unittest.
  `regressions(before, after)` lists everything that got worse.
- **`Session`** is the correction loop:

  ```text
  baseline ← oracle.run()
  loop:
      findings ← scan(project)
      pick the most serious category, then the first rule in pack order
      batch ← its fixable findings that do not overlap
      try the batch:
          apply edits in memory → parse check → binding check
          → formatted files still formatted? (or laid out, nearby only)
          → rescan touched files → targets gone? nothing more serious introduced?
          → write → oracle.run() → no regression?
      accepted: record the step (diff, oracle delta)
      rejected: roll back; one finding → reject it with the reasons;
                several → try the first half alone, the rest come back later
  until nothing fixable remains
  ```

  Every round either accepts or rejects at least one finding, so the loop
  terminates; `max_steps` is a further guard.
- **`formatter`** detects each file's formatter from configuration (rustfmt,
  gofmt, Black, Ruff, Prettier, clang-format), runs it over standard input
  with a time limit, and holds a file to it only if it was formatted before
  the step. `stray_reformatting` keeps `mode = "reformat"` next to the
  correction. Results are cached by file and content hash.
- **`answers`**: `.codemosis/answers.toml`. Questions are identified by a
  hash of rule, file and anchor; an answer is "yes", "no" or the name of one
  of the rule's options.
- **`report`**: terminal output (compiler-style findings), Markdown (for a
  pull request) and JSON (the whole `Report`, serialised).
- **`policy`**: `[[suppress]]`, `[severity]`, `[policy] fail_on` and
  baselines. **`ci`**: SARIF 2.1.0, GitLab Code Quality, GitHub annotations,
  job summaries and pull-request reviews with suggested changes.
- **`proof`**: certificates written by `fix` and verified by replay;
  **`patch`** writes and reverses exact unified diffs; **`lock`** pins packs
  by content; **`audit`** appends every decision to `.codemosis/log.jsonl`;
  **`fsguard`** confines every write to the project.
- The `Workspace` reads the project first and compiles only the packs for
  the grammars it contains (plus text packs), so start-up stays fast.

## codemosis-cli

- **`main`**: the commands, and `FixOptions` shared by `fix` and
  `fix --interactive`.
- **`interactive`**: asks open questions on the terminal one at a time
  (prompts go to stderr, so `--format json` stays machine-readable) and
  records each answer before the next correction round.
- **`map`**: `codemosis map` is the project as the engine sees it: every
  language with its tier and file count, the oracle it detected, and the
  distance per category.
- **`scaffold`**: `codemosis pack new` (a working starter rule for every
  grammar and for text packs) and `codemosis init`.
- **`lsp`**: the language server. **`doctor`**: the environment check.
  **`packdocs`**: generated pack reference pages.

## Adding a language

A language GitHub knows is already recognized; moving it to the lexical or
deep tier is a recipe in the [developer guide](DEVELOPING.md#teach-the-engine-a-language).

## Design decisions

- **Rules are data, the vocabulary is code.** Strangers can write and share
  rules without touching Rust; the facts those rules rely on are a small,
  reviewed, documented vocabulary in the engine.
- **One rule per file, examples inside it.** The proof lives with the claim
  and every change to a rule is a reviewable diff.
- **Anchors, not line numbers.** A finding's identity comes from the rule,
  the file, the enclosing definitions and the code itself, so questions and
  rejections survive unrelated edits.
- **Guessing is refused at every layer.** Language detection says how sure
  it is and rules do not run on a guess; text rules that need to know
  comments from code do not run where the comment syntax is unknown; facts
  that cannot be sure answer in the direction that blocks a correction.
- **In place, on a clean tree.** Corrections are written into the working
  tree so the project's real build and tests can verify them; requiring a
  clean git tree makes `git diff` the review and `git checkout .` the undo.
