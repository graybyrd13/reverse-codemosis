# Changelog

All notable changes are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
follows [Semantic Versioning](https://semver.org/) as described in
[docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

## Unreleased

### Added

- Every language on GitHub (845, from Linguist) in three tiers: 17 with a
  syntax tree, 343 with comment and string syntax, the rest recognized.
- Questions with named answers; `ask`, `fix --interactive`; fixed waits in
  32+ languages; merge conflicts (ours, theirs, both).
- CI outputs: SARIF 2.1.0, GitLab Code Quality, GitHub annotations and job
  summaries, pull-request reviews with suggested changes, versioned JSON.
- Policy: inline `codemosis: ignore[...]` and configured suppressions with
  reasons and expiry dates, baselines, severity levels, `--fail-on`.
- Pack tooling: `pack lint`, `pack test --mutations`, `pack docs`, rule
  metadata (tags, CWE, deprecation), engine ranges, `codemosis lock`.
- Proof certificates for every `fix`, verified by replay with
  `codemosis proof verify`; an audit log.
- Guards: binding preservation (Python), conditional compilation, Rust
  `#[cfg]` and macros, parse damage; soundness, conformance and property
  test suites.
- Parallel analysis, `--timings`, `--log`, `codemosis doctor`.
- A language server, a GitHub Action, pre-commit hooks, a GitLab CI
  component, JSON Schemas, shell completions and man pages.
- Security hardening: sandboxed pack examples, confined writes, redaction.

### Fixed

- Unified diffs had wrong hunk headers in some cases (from `similar`'s
  header computation); the engine now writes them itself.
- Redaction was not idempotent when one match overlapped another.

## 0.1.0

The foundation: the engine, the fact language, fix packs with proofs, the
correction loop verified by the project's build and tests, and the Python
demonstration.
