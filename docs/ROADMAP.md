# Roadmap

The foundation is in place: the engine, the pack format and its proof system,
the verification loop and the demonstration. What follows is ordered by how
much it strengthens correction, which comes before anything else.

## Next

- **Compiler diagnostics as facts.** Read `rustc --error-format=json`,
  `tsc`, `mypy` and friends. A machine-applicable suggestion with a unique
  resolution becomes a proven correction; a suggestion the compiler itself
  marks uncertain becomes a question. Packs can then match on diagnostics as
  well as on shapes.
- **More languages.** TypeScript/JavaScript and Go first, each with a
  runner so its packs prove themselves the way Python and Rust packs do.
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
- **Interactive answering** in the terminal, with the evidence on screen.
- **Structural corrections** across files: moving a definition, splitting a
  module, each step still verified.

## Much later

Design: proposing structure rather than correcting it. It begins only once
correction is trusted completely.
