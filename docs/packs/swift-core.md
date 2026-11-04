# swift-core 0.1.0

> Questions for Swift: fixed waits.

**Languages:** swift · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |

## fixed-wait

**Fixed wait** · `swift-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: poll it with a timeout, wait for the process or the event.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

**Shape**

```scheme
(call_expression
  (simple_identifier) @function
  (call_suffix)
  (#any-of? @function "sleep" "usleep")) @stmt

(call_expression
  (navigation_expression
    target: (simple_identifier) @class
    suffix: (navigation_suffix suffix: (simple_identifier) @method))
  (call_suffix)
  (#eq? @class "Thread")
  (#eq? @method "sleep")) @stmt

(try_expression
  expr: (await_expression
    expr: (call_expression
      (navigation_expression
        target: (simple_identifier) @class
        suffix: (navigation_suffix suffix: (simple_identifier) @method))
      (call_suffix)))
  (#eq? @class "Task")
  (#eq? @method "sleep")) @stmt
```

**Facts that must hold**

- `kind(parent(@stmt)) == 'statements'`
- `!inside(@stmt, 'for_statement', 'while_statement', 'repeat_while_statement', 'lambda_literal')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.swift
+++ b/example.swift
@@ -1,5 +1,4 @@
 func refresh() async throws {
     print("refreshing")
-    try await Task.sleep(nanoseconds: 1_000_000)
     print("refreshed")
 }
```

**Counterexample: 1** (no finding)

```swift
func waitReady() {
    while !ready() {
        sleep(1)
    }
}
```

