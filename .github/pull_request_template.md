## What and why

## How it is proven

<!-- For rules: the examples and counterexamples, and `pack test --mutations` output.
     For engine changes: the tests, and why the acceptance contract still holds. -->

## Checklist

- [ ] `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`
- [ ] `codemosis pack test` and `codemosis pack lint --deny-warnings` (if packs changed)
- [ ] Generated files regenerated if they changed (docs/FACTS.md, docs/LANGUAGES.md, docs/packs, schemas)
- [ ] CHANGELOG.md updated for anything users will notice
