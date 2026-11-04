# Linguist language data

`languages.yml` is GitHub Linguist's list of every language GitHub
recognizes: names, extensions, filenames, interpreters, aliases and editor
modes. `crates/codemosis-core/build.rs` reads it at build time and generates
the language table in `codemosis_core::linguist`, so the binary needs no data
files at run time.

It is copied unmodified from
[github-linguist/linguist](https://github.com/github-linguist/linguist)
(`lib/linguist/languages.yml`) and is distributed under Linguist's MIT
license, reproduced in [LICENSE](LICENSE).

## Updating

1. Replace `languages.yml` with the current file from Linguist.
2. `cargo test --workspace`. Two tests guard the update:
   `every_github_language_is_known` checks that every entry is detected, and
   `every_grammar_owns_its_primary_extension` checks that a renamed or
   re-extensioned language does not silently take files away from a grammar.
   Loading a pack (and so the pack tests) fails if a `patterns` table
   names a language that no longer exists.
3. `cargo run -q -- languages` and update the tier counts in the README if
   they changed.

The content heuristics for ambiguous extensions (Linguist's
`heuristics.yml`) are not embedded; the ones the engine needs are written
out in `crates/codemosis-core/src/linguist.rs`, each with a test.
