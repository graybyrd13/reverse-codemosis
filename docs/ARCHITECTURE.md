# Architecture

```text
            ┌───────────────────────── codemosis-cli ─────────────────────────┐
            │  check · fix · answer · explain · rules · pack test/new · tree  │
            └───────────────┬─────────────────────────────────┬───────────────┘
                            │                                 │
            ┌───────── codemosis-engine ─────────┐   ┌── codemosis-core ─────────────┐
            │ Workspace   config, packs, answers │   │ model    findings, distance   │
            │ Session     the correction loop    │──▶│ pack     manifest + rules     │
            │ Oracle      build + test commands  │   │ rule     compile, match, edit │
            │ testparse   per-test results       │   │ expr     the fact language    │
            │ answers     the human-facts ledger │   │ eval     fact vocabulary      │
            │ report      text, markdown, json   │   │ template rewrite templates    │
            │ vcs         clean-tree check       │   │ lang     python, rust         │
            └────────────────────────────────────┘   │ project  parsed source files  │
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
- **`project`**: a `Project` is every source file of a directory (honouring
  `.gitignore`), each parsed once with tree-sitter. Each file caches one
  language-specific analysis (for Python, the scope analysis), so facts are
  cheap and can look across files.
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
  placement, a snippet runner) with Python (`python.rs`,
  `python_scope.rs`) and Rust (`rust.rs`).
- **`selftest`**: the pack proofs: rewrite to a fixpoint, compare with
  `after`, check idempotence, run before and after with the language runner
  to check the semantics claim, and check counterexamples.

## codemosis-engine

- **`Workspace`** opens a project: `codemosis.toml`, the packs that apply,
  the answers ledger and the parsed source.
- **`Oracle`** runs the project's build and test commands (in the project
  directory, with a time limit, keeping Python bytecode out of the tree) and
  reads per-test results with `testparse` (unittest, pytest, libtest, Go).
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
  hash of rule, file and anchor.
- **`report`**: terminal output (compiler-style findings), Markdown (for a
  pull request) and JSON (the whole `Report`, serialised).

## Adding a language

1. Add the tree-sitter grammar crate to `codemosis-core`.
2. Implement `Language` in `lang/<language>.rs`: id, fact prefix,
   extensions, grammar, comment kinds, `definition_name` (for anchors) and,
   ideally, a `Runner` so pack examples can be executed.
3. Add language facts as the first rules need them, each with a `FactDoc`
   stating its exact meaning and why it is conservative.
4. Register it in `lang::all()`.
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
- **In place, on a clean tree.** Corrections are written into the working
  tree so the project's real build and tests can verify them; requiring a
  clean git tree makes `git diff` the review and `git checkout .` the undo.
