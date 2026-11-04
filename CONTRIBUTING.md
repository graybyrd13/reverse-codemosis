# Contributing

Reverse Codemosis gets stronger with every rule a stranger contributes. This
guide covers rules (the most valuable contribution) and engine code.

## Building and testing

```console
$ cargo build
$ cargo test --workspace          # unit tests, the demo proof, pack self-tests
$ cargo run -q -- pack test       # the built-in packs' proofs, in detail
$ scripts/demo.sh                 # the proof, end to end
```

The tests need `python3`, `rustc` and `node` on the PATH, because pack
examples are executed. The other runners (`go`, `javac`, `cc`, `c++`,
`ruby`, `php`, `bash`, `lua`, `swift`) are used when present; a missing one
turns its semantic checks into reported skips, never passes. Before sending a change:

```console
$ cargo fmt --all
$ cargo clippy --all-targets -- -D warnings
$ cargo test --workspace
```

## Contributing a rule

1. Find the shape: write the bad code in a file and run
   `codemosis tree file.py`.
2. Write the rule in `packs/<pack>/rules/<id>.toml` (see
   [docs/PACK_FORMAT.md](docs/PACK_FORMAT.md)).
3. Write examples that would catch a wrong rule, and counterexamples for
   every case where a careless version of the rule would be wrong.
4. Run `cargo run -q -- pack test --rule <id> -v` until every check passes.
5. Check it against a real project with `codemosis check`.

A rule is accepted when its correction is right in every case its facts
admit, it asks wherever intent decides, its examples prove its semantics
claim by running, and its explanation would satisfy a careful reviewer. "It
is usually right" is not enough: a rule that is usually right is a rule that
asks.

If the rule needs a fact the vocabulary lacks, add the fact to the engine
(`crates/codemosis-core/src/eval.rs` for generic facts,
`crates/codemosis-core/src/lang/<language>.rs` for language facts) with a
`FactDoc` describing its exact meaning, tests in
`crates/codemosis-core/tests/`, and conservative behaviour whenever it is
unsure. Then regenerate the reference in `docs/FACTS.md`:

```console
$ UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo
```

## Teaching the engine a language

Every language GitHub knows is already recognized. Three contributions make
it understand more, from smallest to largest:

- **A wait idiom** (or any other entry in a `patterns` table): one line in
  `packs/waits/tables/waits.toml`, keyed by the GitHub language name, plus
  an example in `packs/waits/rules/fixed-wait.toml`. The pattern must match
  the whole statement on its line and capture it as `call`.
- **Comment and string syntax** (the lexical tier): add the language to a
  profile in `crates/codemosis-core/src/lex.rs`, or map its Linguist
  `ace_mode` to an existing family, with a test in that file showing a
  comment, a string and code told apart.
- **A grammar** (the deep tier): add a row to the table in
  `crates/codemosis-core/src/lang/generic.rs` with the tree-sitter crate,
  its comment kinds, definition kinds and a `CommandRunner` so pack examples
  prove themselves, then start a `<language>-core` pack. Facts specific to
  the language come later, as rules need them.

The language list itself comes from Linguist; see
[data/linguist/README.md](data/linguist/README.md) to update it.

## Changing the engine

- Keep the acceptance contract intact (see
  [docs/SEMANTICS.md](docs/SEMANTICS.md)). A change that could let a step
  through that makes the code worse needs a test proving it cannot.
- If a change alters a demo's output, review `examples/broken-inventory.diff`
  and `examples/flaky-checkout.diff` line by line and regenerate them with
  `UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo`.
- Prefer small, documented functions over cleverness: people need to be able
  to trust this code.

## Licensing

By contributing you agree that your contributions are licensed under both the
MIT license and the Apache License 2.0, as the project is.
