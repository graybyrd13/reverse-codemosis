---
description: Apply the corrections Reverse Codemosis can prove, verified by the build and tests
---

Apply every correction Reverse Codemosis can prove in this project.

1. Call the `fix` tool of the `codemosis` MCP server with no arguments to preview the diff, and show me a
   short summary of it.
2. If I have not objected, call `fix` again with `apply=true` (and `allow_dirty=true` only if I agree to
   mix the corrections with uncommitted changes). Each step is kept only if the build and tests do not
   regress.
3. Show what was applied, anything rejected and why, and the proof certificate's path. If questions remain,
   ask me them one by one and record my answers with the `answer` tool.
