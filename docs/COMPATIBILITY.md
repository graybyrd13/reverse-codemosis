# Compatibility, versions and deprecation

Reverse Codemosis edits code that people depend on, and other tools depend
on its output. This page says what stays stable, for how long, and how
change is announced.

## Versions

The engine, the CLI and the built-in packs share one version and follow
[Semantic Versioning](https://semver.org/). Until 1.0, a minor release
(0.x → 0.x+1) may make breaking changes, always listed under "Breaking" in
[CHANGELOG.md](../CHANGELOG.md); a patch release never does. From 1.0 on,
breaking changes wait for a major release.

## What is a stable interface

| Interface | Identified by | A breaking change is |
|---|---|---|
| Command line | flags, subcommands, exit codes (0 clean, 1 findings or gate tripped, 2 error) | removing or renaming a flag, changing an exit code's meaning |
| JSON outputs | the `schema` field: `codemosis/check/v1`, `codemosis/fix/v1`, `codemosis/languages/v1` | removing or retyping a field (adding fields is not breaking) |
| Files | `codemosis/baseline/v1`, `codemosis/proof/v1`, lockfile `version = 1`, schemas in `schemas/` | anything an older engine could not read |
| SARIF and GitLab reports | SARIF 2.1.0; the `codemosis/v1` fingerprint | changing how fingerprints are computed |
| Pack format | the engine range a pack declares (`engine = "^0.1"`) | anything an existing valid pack could not load or would mean differently |
| Fact vocabulary | `codemosis facts`, docs/FACTS.md | removing a fact or making it less conservative |
| Finding identity | fingerprints and question ids | changing them for unchanged code (it would orphan answers, baselines and suppressions) |

A new schema version is introduced beside the old one, and the engine keeps
reading the previous version for at least one minor release (two after
1.0), converting it when it writes.

## Packs and engines

A pack states the engines it supports (`engine = ">=0.1, <0.3"`); an engine
outside that range refuses the pack with a message rather than misreading
it. `codemosis.lock` pins packs by content, so an engine upgrade that
changes a built-in pack is visible and reviewed.

## Deprecation

Nothing stable is removed without notice:

1. The deprecation is announced in the changelog and, for rules, in the
   rule itself (`deprecated = "why"`, `replaced_by = "pack/rule"`), which
   `explain`, `rules`, SARIF and the pack docs show.
2. The deprecated interface keeps working for at least one minor release
   before 1.0, and at least six months and one major release after.
3. Removal is listed under "Breaking" in the changelog, with the migration.

## Releases and support

Releases are cut from `main` when the [release checklist](RELEASING.md) is
green; there is no fixed schedule before 1.0. Security fixes go to the
latest release (see [SECURITY.md](../SECURITY.md)). A long-term-support line
will be designated with 1.0, receiving security and soundness fixes for
twelve months.

## Minimum Rust version

Building requires the Rust version in `rust-version` (`Cargo.toml`),
checked in CI. Raising it is not a breaking change but is noted in the
changelog.
