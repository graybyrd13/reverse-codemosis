# Threat model

Reverse Codemosis reads untrusted input (repositories, packs, configuration)
and writes source code. This document says what an attacker might control,
what they might want, and what stops them. Each defence has a regression
test; the test is named next to it.

## Assets

1. **The user's files outside the project** must never be written.
2. **The user's secrets** (environment variables, keys in the repository)
   must never be printed, uploaded or handed to pack code.
3. **The project's correctness**: an accepted correction must never make
   the code worse by any measure the engine checks.
4. **The user's machine**: commands that only read (`check`, `map`,
   `explain`, `rules`, `languages`, `lsp`) must never execute code.

## Actors and what they control

| Actor | Controls | Example |
|---|---|---|
| A repository author | every file in the repository, including `codemosis.toml` and `.codemosis/` | a pull request a maintainer checks in CI |
| A pack author | rule files, tables and example code | a pack shared on the internet |
| An editor client | LSP messages | a compromised editor extension |
| A network attacker | nothing directly; the engine makes no network requests | |

## Threats and defences

### Writing outside the project

- **Crafted paths.** Every write goes through `fsguard::write_confined`,
  which refuses absolute paths and `..`. *Test: `fsguard::rejects_escapes`.*
- **Planted symbolic links.** The project walker does not follow links, and
  `write_confined` refuses to write through a link at any level, so a
  repository cannot redirect a correction or an answer file to
  `~/.bashrc`. *Test: `fsguard::rejects_symlinks`.*
- **Pack example paths.** An example's `path` must be a plain relative
  path; packs that say otherwise do not load. *Test:
  `security::pack_example_paths_cannot_escape`.*

### Executing code

- **Rules are data.** A rule is a tree-sitter query, a regular expression
  (Rust's `regex`, linear time, no backtracking), facts from a fixed
  vocabulary and templates. Loading or running a rule cannot execute code.
- **`check`, `map`, `explain`, `lsp` run nothing.** They parse and match
  only. The oracle (build and test commands) runs only in `fix`.
- **`fix` runs the project's build and tests**, from `codemosis.toml` or
  detected from `package.json`, `Makefile` and similar. That is running the
  repository's code, exactly as `npm test` would. Run `fix` only on code
  whose tests you would run; in CI, run it only on trusted branches.
- **`pack test` runs pack examples** with each language's toolchain. They
  run in a scratch directory with standard input closed, a time limit, an
  environment reduced to an allow-list (no tokens or credentials), no core
  dumps and a 64 MB limit on file size. They are not otherwise sandboxed:
  run `pack test` only on packs whose code you would run. *Tests:
  `process::snippets_do_not_see_secrets`,
  `process::snippets_cannot_fill_the_disk`.*

### Leaking secrets

- **Output is redacted.** Messages, questions, evidence, source excerpts
  and report diffs pass through `redact`, which masks private-key bodies,
  cloud and service tokens, credentials in URLs and assigned passwords.
  Reports go to CI logs and pull requests, so the default is to mask
  broadly. *Tests: `redact::tests::*`, `security::findings_never_print_a_secret`.*
- **`git diff` is not redacted**: it is the user's own working tree.
- **The engine sends nothing anywhere.** There is no telemetry and no
  network access.

### Making the code worse

This is the core promise and the acceptance contract in
[SEMANTICS.md](SEMANTICS.md) is the defence: targets gone, nothing more
serious introduced, every file parses, the build and every passing test
still pass, no comment deleted, no overlapping edits, no edits in generated
or vendored files, inside macros or preprocessor conditionals, or on files
whose language is a guess. A malicious pack can still propose a harmful
correction that passes all of that (for example, a "preserved" rewrite that
the project's tests do not cover). That is why packs are reviewed like code,
why `pack test` proves each rule's semantics claim, and why the final
review is a diff a person reads line by line.

### Denial of service

- Files over 2 MB and binary files are not read.
- Regular expressions are linear-time; tree-sitter queries are bounded by
  the file.
- Every external command has a time limit.
- A correction session stops after `--max-steps` steps.

### Supply chain

- Release binaries are built in GitHub Actions with build provenance
  attestations, SHA-256 checksums and a CycloneDX SBOM
  (`.github/workflows/release.yml`).
- `cargo deny` checks every dependency for known vulnerabilities, licences
  and sources on every pull request (`deny.toml`), and Dependabot proposes
  updates weekly.
- Grammars are pinned to exact crate versions in `Cargo.lock`, and
  `codemosis languages --format json` reports the version of each.
- Packs can be pinned by content hash in `codemosis.lock`; a pack whose
  files changed fails to load until the lock is updated on purpose.

## Out of scope

- A user who runs `fix` or `pack test` on code they do not trust, after the
  documentation above. Use a container.
- Attacks that need write access to the user's home directory or to the
  `codemosis` binary itself.
