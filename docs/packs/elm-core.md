# elm-core 0.1.0

> Questions for Elm: Debug.log left in code.

**Languages:** elm · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`debug-log`](#debug-log) Debug.log left in code | waste | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |

## debug-log

**Debug.log left in code** · `elm-core/debug-log`

category: waste · action: asks, then corrects as chosen (preserved) · tags: debugging

`Debug.log label value` prints `value` and returns it unchanged, which is how Elm code is debugged, and
`elm make --optimize` refuses any module that still uses `Debug`. Removing the log leaves `value` in its
place, so the program computes the same thing. Some logs are wanted in development builds, so the engine
asks: keep records it as intentional; remove replaces the call with its value.

**Shape**

```scheme
(function_call_expr
  target: (value_expr name: (value_qid (upper_case_identifier) @module (lower_case_identifier) @function))
  arg: (_) @_label
  arg: (_) @value
  (#eq? @module "Debug")
  (#eq? @function "log")) @call
```

**Question:** Keep this Debug.log?

- **keep**: it is wanted in development builds; record it as intentional
- **remove**: replace the call with the value it logs

**Example: 1** (answer: remove)

```diff
--- a/example.elm
+++ b/example.elm
@@ -3,4 +3,4 @@
 
 total : List Int -> Int
 total items =
-    Debug.log "total" (List.sum items)
+    (List.sum items)
```

**Counterexample: 1** (no finding)

```text
module Cart exposing (total)


total : List Int -> Int
total items =
    List.sum items
```

