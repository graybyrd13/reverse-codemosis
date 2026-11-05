# Working on Reverse Codemosis

Guidance for coding agents (and people) changing this repository. The [developer guide](docs/DEVELOPING.md)
has the full map; this is what to know before the first edit.

## What it is

A Rust workspace: `crates/codemosis-core` finds problems and changes nothing on disk;
`crates/codemosis-engine` changes code, verified (correction loop, oracle, formatters, proofs);
`crates/codemosis-cli` is the `codemosis` binary (CLI, LSP server, MCP server, hook). Rules live in
`packs/*/rules/*.toml` and are embedded at build time. Editor and agent integrations are in `editors/` and
`integrations/`.

The project's promise is that it never guesses: a correction is applied only when a rule proves it and the
project's own build and tests agree. Every change must keep that promise.

## Before you send a change

```console
$ cargo fmt --all
$ cargo clippy --all-targets -- -D warnings
$ cargo test --workspace
$ cargo run -q -- pack test                                  # if packs changed
$ cargo run -q -- pack lint --deny-warnings                  # if packs changed
$ cargo run -q -- pack test --mutations --max-unproven 0     # if rule facts changed
$ UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo    # regenerates docs/FACTS.md, LANGUAGES.md, docs/packs, schemas
$ scripts/field-test.sh --compare scripts/field-test-baseline.json   # if rules or the engine changed
```

## Rules of the house

- **Soundness first.** A missed correction is acceptable; a wrong one is not. Never remove a guard, loosen a
  fact or weaken a test to make something pass. Every `where` fact needs a counterexample that fails
  without it (CI enforces `--max-unproven 0`); an engine guard needs a case in
  `crates/codemosis-engine/tests/soundness.rs`.
- **Questions belong to the user.** Rules that cannot prove a change ask (`[ask]`); code that answers on
  the user's behalf is a bug.
- **Generated files** (`docs/FACTS.md`, `docs/LANGUAGES.md`, `docs/packs/`, `schemas/`) are rewritten by
  the demo test with `UPDATE_GOLDEN=1`; do not edit them by hand.
- **Golden diffs** in `examples/*.diff` change only when a correction deliberately changes; review them.
- **Docs move with code**: user-visible changes need `CHANGELOG.md` and the relevant guide in `docs/`.
- Run the tool on itself: `cargo run -q -- check crates`.
