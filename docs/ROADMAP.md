# Roadmap

The engine, the pack format and its proof system, the verification loop,
every language GitHub knows in three tiers, questions with named answers,
CI and editor integrations, proof certificates and the soundness guards are
in place. What follows is ordered by how much it strengthens correction,
which comes before everything else.

## Next

- **Prove more facts necessary.** `codemosis pack test --mutations`
  reports 52 of 140 facts in the built-in packs as proven necessary by a
  counterexample. Every unproven fact gets a counterexample or is removed;
  releases may not lower the number.
- **Binding analysis beyond Python.** JavaScript/TypeScript, Go, Java and
  Rust scope graphs, so their packs can prove removals and renames and the
  binding-preservation guard covers them.
- **Compiler diagnostics as facts.** Read `rustc --error-format=json`,
  `tsc`, `mypy` and friends; a machine-applicable suggestion with a unique
  resolution becomes a proven correction, an uncertain one a question.
- **Type-aware facts**, backed by a type checker, to unlock corrections
  that are blocked only because a type cannot be established syntactically.
- **Multi-file examples** in packs, so cross-file facts are proven in packs
  and not only in engine tests.

## Later

- **Incremental analysis and a content-addressed cache** keyed by file
  hash, grammar, pack digest and engine version, for monorepos and for a
  `codemosis watch` daemon that keeps findings current between commands.
- **Pack registry**: discovery, signed releases (the lockfile already pins
  content), compatibility checks against engine ranges, security review.
- **Constraint checking with an SMT solver** for facts that are arithmetic
  or boolean in nature, with counterexamples reported like any other
  refusal.
- **Out-of-tree grammars** through a stable plugin interface (WebAssembly,
  with no host access), so the deep tier can grow without rebuilding.
- **A playground** in the browser: paste code, pick packs, see facts,
  findings and proofs.
- **Structural corrections across files**: moving a definition, splitting a
  module, each step still verified.

## Much later

Design: proposing structure rather than correcting it. It begins only once
correction is trusted completely.
