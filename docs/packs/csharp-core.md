# csharp-core 0.1.0

> Questions and explanations for C#.

**Languages:** csharp · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`empty-catch`](#empty-catch) Empty catch block | hazard | explains | 1 ex. · 2 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 3 counter |

## empty-catch

**Empty catch block** · `csharp-core/empty-catch`

category: hazard · action: explains · CWE-390

An empty catch hides every exception of that type, including the unexpected
ones. When ignoring is right, a comment saying why makes it a decision (and
the rule stays quiet); otherwise handle, log or rethrow.

**Shape**

```scheme
(catch_clause body: (block) @body) @clause
```

**Facts that must hold**

- `count(children(@body)) == 0`
- `!has_comment(@body)`

**Example: 1**

```csharp
class Loader {
    void Load() {
        try { Read(); } catch (Exception) { }
    }
}
```

**Counterexample: 1** (no finding)

```csharp
class Loader {
    void Load() {
        try { Read(); } catch (Exception) { /* optional */ }
    }
}
```

**Counterexample: a catch that handles the exception** (no finding)

```csharp
class Loader {
    void Load() {
        try {
            Read();
        } catch (System.Exception e) {
            System.Console.Error.WriteLine(e);
        }
    }
    void Read() {}
}
```


## fixed-wait

**Fixed wait** · `csharp-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: join the thread, wait on the channel or future, poll with a timeout.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Shape**

```scheme
(expression_statement
  (invocation_expression
    function: (member_access_expression expression: (identifier) @class name: (identifier) @method)) @call
  (#eq? @class "Thread")
  (#eq? @method "Sleep")) @stmt

(expression_statement
  (await_expression
    (invocation_expression
      function: (member_access_expression expression: (identifier) @class name: (identifier) @method))) @call
  (#eq? @class "Task")
  (#eq? @method "Delay")) @stmt
```

**Facts that must hold**

- `file_purpose() not in ['example', 'docs', 'benchmark']`
- `!inside(@stmt, 'for_statement', 'foreach_statement', 'while_statement', 'do_statement', 'lambda_expression')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/Example.cs
+++ b/Example.cs
@@ -1,7 +1,6 @@
 class Checkout {
     async Task Pay() {
         Charge();
-        await Task.Delay(2000);
         Confirm();
     }
 }
```

**Counterexample: 1** (no finding)

```csharp
class Poller {
    async Task Wait() {
        while (!Ready()) {
            await Task.Delay(100);
        }
    }
}
```

**Counterexample: the same wait in an example: pausing there is usually the point** (no finding)

```csharp
class Checkout {
    async Task Pay() {
        Charge();
        await Task.Delay(2000);
        Confirm();
    }
}
```

**Counterexample: a wait that shares its line** (no finding)

```csharp
using System.Threading;
class Worker {
    void Settle() { Flush(); Thread.Sleep(100); }
    void Flush() {}
}
```

