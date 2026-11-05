---
description: Check code with Reverse Codemosis (default: everything changed since the last commit)
argument-hint: "[files...]"
---

Check the code with Reverse Codemosis. If files were given ($ARGUMENTS), check those; otherwise check every
file changed since the last commit. Use the `check` tool of the `codemosis` MCP server, or run
`codemosis check --changed --format compact` (or `--file <path>` for each file given).

Then report back, grouped:

1. **Proven corrections**: say what each changes, and offer to apply them all with the `fix` tool (preview
   first, then `apply=true`).
2. **Questions**: these are my decisions. Ask me each one with its options; record my answer with the
   `answer` tool. Never pick an answer for me.
3. **Explained risks**: no provable correction exists; explain the risk briefly.
