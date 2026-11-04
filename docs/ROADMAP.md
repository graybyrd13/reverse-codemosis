# Roadmap

The foundation is in place: the engine, the pack format and its proof system,
the verification loop, every language GitHub knows in three tiers, questions
with named answers, and two demonstrations. What follows is ordered by how
much it strengthens correction, which comes before anything else.

## Next

- **Compiler diagnostics as facts.** Read `rustc --error-format=json`,
  `tsc`, `mypy` and friends. A machine-applicable suggestion with a unique
  resolution becomes a proven correction; a suggestion the compiler itself
  marks uncertain becomes a question. Packs can then match on diagnostics as
  well as on shapes.
- **Deeper facts for the new grammars.** JavaScript, Go, Java and the
  others have shapes and runners; scope analysis like Python's would let
  their packs prove removals and renames, not only local rewrites.
- **Type-aware facts.** Many corrections are blocked only because a fact
  such as "this operand is a str" cannot be established syntactically.
  Facts backed by a type checker would unlock them while staying
  conservative.
- **Multi-file examples.** Let pack examples describe several files, so
  cross-file facts (`py.imported_elsewhere`, `py.defined_in_project`) are
  proven in packs, not only in engine tests.
- **CI mode.** `codemosis check --fail-on contradiction` and SARIF output, so
  the distance can be held at zero in a pipeline.

## Later

- **Pack registry.** Discover, version, sign and share packs; pin pack
  versions in `codemosis.toml`.
- **Incremental scans** for large repositories, re-analysing only what a
  step touched.
- **Editor integration** through the Language Server Protocol: findings,
  questions and proven corrections where the code is written.
- **Structural corrections** across files: moving a definition, splitting a
  module, each step still verified.

## Much later

Design: proposing structure rather than correcting it. It begins only once
correction is trusted completely.
