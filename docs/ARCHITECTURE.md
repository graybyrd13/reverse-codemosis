# Architecture

```text
            ┌───────────────────────── codemosis-cli ─────────────────────────┐
            │ check · fix · ask · answer · explain · map · languages · rules  │
            │ pack test/new · tree · facts · init                             │
            └───────────────┬─────────────────────────────────┬───────────────┘
                            │                                 │
            ┌───────── codemosis-engine ─────────┐   ┌── codemosis-core ─────────────┐
            │ Workspace   config, packs, answers │   │ model    findings, distance   │
            │ Session     the correction loop    │──▶│ pack     manifest + rules     │
            │ Oracle      build + test commands  │   │ rule     compile, match, edit │
            │ testparse   per-test results       │   │ expr     the fact language    │
            │ answers     the human-facts ledger │   │ eval     fact vocabulary      │
            │ report      text, markdown, json   │   │ template rewrite templates    │
            │ vcs         clean-tree check       │   │ lang     17 grammars          │
            └────────────────────────────────────┘   │ linguist 845 languages        │
                                                     │ lex      code/comment/string  │
                                                     │ project  every source file    │
                                                     │ selftest pack proofs          │
                                                     └───────────────────────────────┘
                                     packs/  (embedded at build time by build.rs)
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
  `after`, check idempotence, run before and after with the language runner
  to check the semantics claim, and check counterexamples.

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
          apply edits in memory → parse check → rescan touched files
          → targets gone? nothing more serious introduced?
          → write → oracle.run() → no regression?
      accepted: record the step (diff, oracle delta)
      rejected: roll back; one finding → reject it with the reasons;
                several → try the first half alone, the rest come back later
  until nothing fixable remains
  ```

  Every round either accepts or rejects at least one finding, so the loop
  terminates; `max_steps` is a further guard.
- **`answers`**: `.codemosis/answers.toml`. Questions are identified by a
  hash of rule, file and anchor; an answer is "yes", "no" or the name of one
  of the rule's options.
- **`report`**: terminal output (compiler-style findings), Markdown (for a
  pull request) and JSON (the whole `Report`, serialised).

## codemosis-cli

- **`main`**: the commands, and `FixOptions` shared by `fix` and
  `fix --interactive`.
- **`interactive`**: asks open questions on the terminal one at a time
  (prompts go to stderr, so `--format json` stays machine-readable) and
  records each answer before the next correction round.
- **`map`**: `codemosis map` is the project as the engine sees it: every
  language with its tier and file count, the oracle it detected, and the
  distance per category.
- **`scaffold`**: `codemosis pack new` and `codemosis init`.

## Adding a language

A language GitHub knows is already recognized. To move it up a tier:

- **lexical**: add its comment and string syntax to a profile in
  `core/src/lex.rs` (or map its Linguist `ace_mode` to an existing family).
- **deep**:

1. Add the tree-sitter grammar crate to `codemosis-core`.
2. Implement `Language` in `lang/<language>.rs`: id, fact prefix,
   extensions, grammar, comment kinds, `definition_name` (for anchors) and,
   ideally, a `Runner` so pack examples can be executed.
3. Add language facts as the first rules need them, each with a `FactDoc`
   stating its exact meaning and why it is conservative.
4. Register it in `lang::all()` (or add a row to the table in
   `lang/generic.rs` when it needs no facts of its own yet).
5. Write a `<language>-core` pack.

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
