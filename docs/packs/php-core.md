# php-core 0.1.0

> Questions for PHP: fixed waits and debugging output left behind.

**Languages:** php · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`debug-dump`](#debug-dump) Debugging dump left in the code | waste | asks, then corrects as chosen (corrected) | 1 ex. · 2 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 3 counter |

## debug-dump

**Debugging dump left in the code** · `php-core/debug-dump`

category: waste · action: asks, then corrects as chosen (corrected) · CWE-489

`var_dump`, `print_r`, `dump` and `dd` are how PHP debugging starts, and
they are often left behind, leaking internal data into pages and API
responses (`dd` also stops the request). Some are deliberate (a debugging
script), so the engine asks.

**Shape**

```scheme
(expression_statement
  (function_call_expression function: (name) @function) @call
  (#any-of? @function "var_dump" "print_r" "dump" "dd" "var_export")) @stmt
```

**Facts that must hold**

- `occupies_lines(@stmt)`

**Question:** Keep this debugging output?

- **keep**: it is intended output; record it as intentional
- **remove**: it is leftover debugging; delete it

**Example: 1** (answer: remove)

```diff
--- a/example.php
+++ b/example.php
@@ -1,4 +1,3 @@
 <?php
 $total = array_sum([1, 2, 3]);
-var_dump($total);
 echo $total, "\n";
```

**Counterexample: 1** (no finding)

```php
<?php
echo json_encode(["ok" => true]);
```

**Counterexample: a dump that shares its line** (no finding)

```php
<?php
function total($xs) { var_dump($xs); return array_sum($xs); }
```


## fixed-wait

**Fixed wait** · `php-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: poll it with a timeout, wait for the process or the event.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Shape**

```scheme
(expression_statement
  (function_call_expression function: (name) @function) @call
  (#any-of? @function "sleep" "usleep" "time_nanosleep")) @stmt
```

**Facts that must hold**

- `file_purpose() not in ['example', 'docs', 'benchmark']`
- `!inside(@stmt, 'for_statement', 'foreach_statement', 'while_statement', 'do_statement')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.php
+++ b/example.php
@@ -1,4 +1,3 @@
 <?php
 echo "start\n";
-usleep(1000);
 echo "done\n";
```

**Counterexample: 1** (no finding)

```php
<?php
while (!file_exists("ready")) {
    usleep(100000);
}
```

**Counterexample: the same wait in an example: pausing there is usually the point** (no finding)

```php
<?php
echo "start\n";
usleep(1000);
echo "done\n";
```

**Counterexample: a wait that shares its line** (no finding)

```php
<?php
function settle() { flush(); sleep(1); }
```

