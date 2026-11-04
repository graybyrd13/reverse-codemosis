# Security policy

Reverse Codemosis edits source code, so its users trust it with their
repositories. Security reports are handled ahead of all other work.

## Reporting a vulnerability

Please report vulnerabilities privately, through GitHub's
[private vulnerability reporting](https://github.com/graybyrd13/reverse-codemosis/security/advisories/new)
for this repository. Do not open a public issue for a vulnerability.

Include what you can of:

- the version (`codemosis --version`) and platform;
- what an attacker controls (a repository being checked, a pack, a
  configuration file, an editor connection...);
- what they gain (writing outside the project, running code during
  `check`, reading a secret, making `fix` accept a harmful change...);
- a minimal reproduction.

## What happens next

| When | What |
|---|---|
| 3 working days | We acknowledge the report and say who is handling it. |
| 10 working days | We confirm or rule out the issue and agree a severity (CVSS 4.0) with you. |
| 90 days at most | A fix is released, with a GitHub Security Advisory and a CVE for anything rated medium or above. We will ask before publishing your name. |

Fixes go to the latest release. While the project is below 1.0 there is
no backport to older versions; see [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md).

Advisories are published in this repository's
[security advisories](https://github.com/graybyrd13/reverse-codemosis/security/advisories),
which GitHub also serves as a machine-readable feed and forwards to the
RustSec advisory database for the published crates.

## Scope

The threat model, with what is and is not defended against, is in
[docs/THREAT_MODEL.md](docs/THREAT_MODEL.md). In short: `check`, `map`,
`explain` and the language server must never execute code from the
repository or from a pack, and no command may write outside the project.
A way to break either of those is a vulnerability. `fix` runs the
project's own build and tests by design, and `pack test` runs pack
examples by design; those are documented, not vulnerabilities.
