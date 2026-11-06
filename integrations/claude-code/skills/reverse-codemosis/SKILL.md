---
name: reverse-codemosis
description: Use when checking, correcting or reviewing code in a project that uses Reverse Codemosis (a codemosis.toml file, a `codemosis` MCP server, or reports from `codemosis hook`), or when the user wants code fixed only where the fix can be proven.
---

# Working with Reverse Codemosis

Reverse Codemosis has no language model inside and never guesses. Every finding has one of three kinds, and
each asks for a different response.

**Proven correction** (`fixable`). A rule proves the change is safe. Make exactly the change shown, or call
`fix` (preview, then `apply=true`); `fix` keeps a step only if the project's build and tests do not regress,
and writes a proof certificate. Do not "improve" a proven correction: the proof covers that change only.

**Question** (`question`, or "a decision for the user"). Only the user can decide, for example whether a
fixed `sleep` is a deliberate rate limit. Ask the user, showing the options. Record the answer they give
with `answer` (or `codemosis answer <id> <answer>`). Never choose for them, and never add an ignore comment
to make a question go away.

**Explained** (`explained`). There is a real risk but no provable correction. Tell the user what it is and
where; propose a change only as a suggestion they review.

After you edit code, `codemosis hook` (if installed) reports findings on the lines you changed. Treat that
report the same way. To check yourself: the `check` tool with the files you changed, or
`codemosis check --changed --format compact`.

`codemosis explain <rule>` explains any rule, including the facts that must hold before it corrects.
