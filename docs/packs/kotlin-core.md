# kotlin-core 0.1.0

> Questions for Kotlin: fixed waits.

**Languages:** kotlin · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |

## fixed-wait

**Fixed wait** · `kotlin-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: poll it with a timeout, wait for the process or the event.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported. Waits inside lambdas (`repeat(n) { }`) are treated as loops.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Shape**

```scheme
(call_expression
  (navigation_expression (identifier) @class (identifier) @method)
  (value_arguments)
  (#eq? @class "Thread")
  (#eq? @method "sleep")) @stmt

(call_expression
  (identifier) @method
  (value_arguments (value_argument (number_literal)))
  (#eq? @method "delay")) @stmt
```

**Facts that must hold**

- `file_purpose() not in ['example', 'docs', 'benchmark']`
- `kind(parent(@stmt)) in ['block', 'statements', 'source_file']`
- `!inside(@stmt, 'for_statement', 'while_statement', 'do_while_statement', 'lambda_literal')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.kt
+++ b/example.kt
@@ -1,5 +1,4 @@
 suspend fun checkout() {
     charge()
-    delay(2000)
     confirm()
 }
```

**Counterexample: 1** (no finding)

```kotlin
fun waitReady() {
    while (!ready()) {
        Thread.sleep(100)
    }
}
```

