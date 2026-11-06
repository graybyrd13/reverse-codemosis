# Releasing

A release is cut only when every item below holds. The release workflow
(`.github/workflows/release.yml`) automates the build; this checklist is
what a maintainer confirms before pushing the tag.

## Qualification

- [ ] CI is green on `main`: format, clippy, tests on Linux and macOS, the
      MSRV build, API docs, cargo-deny, the action, the demonstrations.
- [ ] `codemosis pack test` passes with every runner installed (no skips on
      the release machine), and `codemosis pack lint --deny-warnings` is clean.
- [ ] `codemosis pack test --mutations`: the number of facts proven
      necessary did not go down since the last release.
- [ ] The soundness suite, conformance suite and property tests pass, the
      property tests with `CODEMOSIS_PROPTEST_CASES=50000`.
- [ ] Both demonstrations reproduce their golden diffs, and their proof
      certificates verify (`codemosis proof verify`).
- [ ] `codemosis check crates` on this repository is clean.
- [ ] `codemosis check --timings` on a large repository is no slower than
      the previous release by more than 10%.
- [ ] Generated files are current: docs/FACTS.md, docs/LANGUAGES.md,
      docs/packs/, schemas/ (the tests check them).
- [ ] No open security advisory affects the release.

## Compatibility

- [ ] Every breaking change is under "Breaking" in CHANGELOG.md with its
      migration (docs/COMPATIBILITY.md).
- [ ] Schema versions changed wherever a file or output format changed.
- [ ] Built-in packs' `engine` ranges include the new version.

## Cutting it

1. Set the version in `Cargo.toml` (`[workspace.package]`) and the
   internal dependency versions; run `cargo build` to update Cargo.lock.
2. Move "Unreleased" in CHANGELOG.md under the new version and date.
3. Commit, then tag: `git tag -s v0.x.y -m "v0.x.y"` and push the tag.
4. The release workflow builds Linux, macOS and Windows binaries, writes
   SHA-256 checksums and a CycloneDX SBOM, attests build provenance, and
   drafts the GitHub release with notes grouped by label.
5. Check the draft (binaries run, `codemosis --version`, checksums), then
   publish it.
