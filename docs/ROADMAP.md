# Roadmap

Where Reverse Codemosis goes next, in order. Each milestone has a goal, the
evidence that motivates it, deliverables, and exit criteria that can be
measured. Correction comes before features: no milestone may lower the
[field test](FIELD_TEST.md)'s precision.

## Where things stand

The engine, the pack format and its proofs, the verification loop, all 845
GitHub languages in three tiers, questions, CI and editor integrations,
proof certificates and the soundness guards are in place. The
[field test](FIELD_TEST.md) on 20 open-source projects found 127 proven
corrections (all correct), 128 legitimate questions and three genuine bugs,
and exposed what to do next:

| Evidence from the field test | Leads to |
|---|---|
| A correct ripgrep correction breaks its 79-column `rustfmt` check | formatter-aware corrections (M1, **done**) |
| Projects' test suites were not run as the oracle (third-party code) | a sandboxed field-test job (M1, **built**) |
| 105 of 160 facts were not proven necessary by any counterexample | proof coverage (M1, **141 of 144 now**) |
| `bool(...)` and `not (a != b)` are kept because types are unknown | type facts (M2) |
| Binding analysis exists only for Python | scope graphs for more languages (M2) |
| 42 files hit grammar gaps (C macros, Kotlin `actual`, Swift `()`, POSIX `case`) | grammar work (M2; zsh done) |
| Django takes 7.4 s cold, every time | incremental analysis (M3) |

## M1: verified on real projects (0.2)

**Goal:** every correction passes the project's own test suite *and*
formatter, demonstrated continuously on real repositories.

1. **Formatter-aware acceptance (done).** Detect each project's formatter: rustfmt
   (`rustfmt.toml`, Cargo projects), gofmt (always for Go), Black or Ruff
   format (`pyproject.toml`), Prettier (its config files) and clang-format
   (`.clang-format`). If a touched file was formatter-clean before a step,
   it must be clean after it. The step reformats only the lines it changed
   when the formatter supports ranges, and is refused otherwise. Add this to
   the acceptance contract and the proof certificate.
   *Exit:* the ripgrep correction is reformatted to 79 columns, or
   refused; a soundness test covers each formatter; no change to the two
   demonstrations' diffs.
   *Delivered:* `engine/src/formatter.rs`, `[format] mode = "refuse" |
   "reformat" | "off"`; reformatting is kept within three lines of the
   correction and recorded in the certificate; `proof verify` replays it.
   Both ripgrep outcomes are confirmed. Open: range formatting where a
   formatter supports it (clang-format `--lines`, Prettier
   `--range-start`), so a formatter that would also rewrite distant lines
   can still lay out the correction alone.
2. **Field test in CI, with real oracles (built; first run green).** A weekly workflow
   (`.github/workflows/field-test.yml`) runs `codemosis fix` on the pinned
   repositories inside per-ecosystem containers (dependencies installed,
   then the network cut off), with each project's own test command as the
   oracle, then `proof verify`. It publishes the results table and fails on
   any accepted step that regressed a test.
   *Exit:* at least six repositories in four ecosystems run end to end;
   zero accepted regressions over four consecutive weeks.
   *Delivered:* `scripts/field-test-oracle.sh` and the `oracle` job for
   cobra, gin and ripgrep (Go and Cargo: dependencies fetched, then the
   package managers offline); each run ends with `proof verify`, an
   independent re-run of the tests and the formatter check. First run: three
   corrections kept with 429, 733 and 446 tests unchanged, one refused by
   the formatting contract, every certificate verified. Next: Python
   and JavaScript projects (requests, click, express), which need their
   test dependencies installed per project.
3. **Precision ledger (done).** `scripts/field-test.sh` writes a machine-readable
   summary, and CI compares it with a checked-in baseline: questions per
   thousand files, explanations per thousand files, proven corrections,
   and corrections reviewed correct. A rise in noise needs a stated reason
   in the pull request.
   *Exit:* the baseline is committed, and CI fails on an unexplained
   increase of more than 10%.
   *Delivered:* `--summary-json` and `--compare`;
   `scripts/field-test-baseline.json` holds all 127 reviewed corrections by
   fingerprint, so any new proven correction fails CI until someone has read
   it.
4. **Proof coverage (nearly done).** Every `where` fact in the built-in packs either has
   a counterexample that needs it (`pack test --mutations`) or is removed.
   *Exit:* every fact proven necessary, enforced in CI.
   *Delivered:* 141 of 144 (from 55 of 160). 98 counterexamples were
   added. Writing them showed that 16 facts were implied by others
   (for example, `exists(@x)` before `kind(@x)`, a shape predicate repeated
   in `where`, or a line-anchored pattern). Those facts were removed, each
   with a comment, and where a table makes a fact redundant a test in
   `codemosis-core/tests/pack_invariants.rs` keeps the table so. CI runs
   `pack test --mutations --max-unproven 3`. The three left are cross-file
   facts (`py.defined_in_project`, `py.imported_elsewhere`), which need
   multi-file examples (M2.4).
5. **Readable corrections.** Offer the clearer form when it is provably
   equal: `a not in b` instead of `not (a in b)` (already done), and `==`
   instead of `not (... != ...)` when *both* operands are of builtin types.
   A literal on one side is not enough: Python asks the left operand's
   `type(a).__ne__` first, so `a` of a class whose `__ne__` disagrees with
   its `__eq__` decides, whatever the literal. (An earlier version of this
   plan said one literal sufficed; that would have been unsound.) This
   needs `py.type_of` (M2.2) for anything but literal-against-literal.
   *Exit:* `not (x != y)` becomes `x == y` exactly when both types are
   known builtins, with a counterexample for a custom `__ne__` on either
   side; Django's `not (engine != "InnoDB")` stays until `engine` is known
   to be a `str`.

## M2: deeper understanding (0.3)

**Goal:** more corrections become provable, across more languages.

1. **Scope graphs beyond Python.** JavaScript/TypeScript (hoisting, TDZ,
   imports), Go, Java and Rust, extending the binding-preservation guard
   to them and enabling proven unused-import and unreachable-code rules
   there.
   *Exit:* the soundness suite has a binding case per language;
   `unused-import` ships for JavaScript, TypeScript and Go.
2. **Compiler and type-checker diagnostics as facts.** Read
   `rustc --error-format=json`, `tsc --noEmit`, `mypy` and `go vet -json`.
   A suggestion the compiler marks machine-applicable with a single
   resolution becomes a proven correction; an uncertain one becomes a
   question. Type facts (`py.type_of`, `ts.type_of`) unlock the `bool()`
   and `!=` cases above.
   *Exit:* the redundant `bool()` is dropped exactly when the operands are
   known to be bools; rustc's machine-applicable fixes pass the field test.
3. **Grammar gaps.** ~~Detect zsh and send it to the lexical tier~~ (done:
   `lang::unread_dialect`); upgrade the Swift and Kotlin grammars, or
   contribute fixes upstream (including tree-sitter-bash's POSIX `case`
   patterns); let packs declare C macros that act as statements or loops
   (`jv_array_foreach`).
   *Exit:* parse damage in the field test drops from 44 files (42 now) to
   under 10.
4. **Multi-file pack examples**, so cross-file facts are proven in packs.

## M3: scale and flow (0.4)

**Goal:** fast enough to run on every keystroke and every commit of a
monorepo.

1. **Incremental analysis with a content-addressed cache**, keyed by file
   hash, grammar version, pack digest and engine version, shared by
   `check`, `lsp` and a new `codemosis watch`.
   *Exit:* re-checking Django after a one-file edit takes under 300 ms;
   a cold check under 4 s on four cores.
2. **Cheaper rules:** merge the queries of rules that share a root node
   kind, and push cheap textual tests into query predicates.
3. **Upstream reporting:** `codemosis report` turns a genuine finding in a
   dependency or another project into issue text with the evidence and
   the rule's explanation, for a person to file.

## M4: an ecosystem (0.5 and later)

- **A pack registry:** discovery, signed releases (the lockfile already
  pins content), checks against engine ranges, security review.
- **Out-of-tree grammars** through a WebAssembly plugin interface with no
  host access.
- **A browser playground:** paste code, pick packs, see facts, findings and
  proofs.
- **SMT-backed facts** for arithmetic and boolean conditions, with
  counterexamples reported like any other refusal.
- **Structural corrections across files** (moving a definition, splitting a
  module), each step still verified.

## The next two weeks

Concrete, ordered, each small enough for one pull request:

1. ~~`engine/src/formatter.rs`~~ (done: all six formatters, refuse and
   reformat).
2. ~~`.github/workflows/field-test.yml`~~ (built for cobra, gin and
   ripgrep; watch its first runs).
3. ~~`scripts/field-test.sh --summary-json` and a committed baseline~~ (done).
4. ~~Counterexamples for the unproven facts~~ (141 of 144; the rest wait
   for multi-file examples, M2.4).
5. ~~zsh detection and routing to the lexical tier~~ (done).
6. Decide on the three upstream reports in [FIELD_TEST.md](FIELD_TEST.md).

## Decisions for the maintainers

- **Formatter policy:** both are implemented; `refuse` is the default and
  `reformat` is opt-in. Should `reformat` become the default once it has
  run on the field test for a release?
- **Upstream reports:** file the three genuine issues found in rich and
  Django, crediting the tool, or keep the field test private until 0.2.
- **0.2 scope:** ship M1 alone, or wait for M2.1 (scope graphs for
  JavaScript), which would make the JavaScript pack substantially stronger.

## Much later

Design: proposing structure rather than correcting it. It begins only once
correction is trusted completely.
