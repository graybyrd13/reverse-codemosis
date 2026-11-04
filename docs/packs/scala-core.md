# scala-core 0.1.0

> Questions for Scala: fixed waits.

**Languages:** scala · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |

## fixed-wait

**Fixed wait** · `scala-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: poll it with a timeout, wait for the process or the event.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

**Shape**

```scheme
(call_expression
  function: (field_expression value: (identifier) @class field: (identifier) @method)
  (#eq? @class "Thread")
  (#eq? @method "sleep")) @stmt
```

**Facts that must hold**

- `kind(parent(@stmt)) in ['block', 'template_body', 'compilation_unit']`
- `!inside(@stmt, 'while_expression', 'for_expression', 'do_while_expression', 'lambda_expression')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.scala
+++ b/example.scala
@@ -1,7 +1,6 @@
 object Checkout {
   def pay(): Unit = {
     charge()
-    Thread.sleep(500)
     confirm()
   }
 }
```

**Counterexample: 1** (no finding)

```scala
object Poller {
  def await(): Unit = {
    while (!ready()) {
      Thread.sleep(100)
    }
  }
}
```

