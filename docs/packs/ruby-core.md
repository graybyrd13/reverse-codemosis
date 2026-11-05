# ruby-core 0.1.0

> Questions for Ruby: fixed waits and breakpoints left behind.

**Languages:** ruby · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`breakpoint`](#breakpoint) Breakpoint left in the code | waste | asks, corrects on yes (corrected) | 2 ex. · 1 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |

## breakpoint

**Breakpoint left in the code** · `ruby-core/breakpoint`

category: waste · action: asks, corrects on yes (corrected) · CWE-489

`binding.pry`, `binding.irb`, `byebug` and `debugger` open a debugging
session when reached. Left in the code, they freeze a server or a test run,
or raise NoMethodError where the debugging gem is not loaded. They are
nearly always leftovers, but the engine asks before removing one.

**Shape**

```scheme
(call receiver: (identifier) @receiver method: (identifier) @method
  (#eq? @receiver "binding")
  (#any-of? @method "pry" "irb" "break")) @stmt

((identifier) @stmt
  (#any-of? @stmt "byebug" "debugger"))
```

**Facts that must hold**

- `kind(parent(@stmt)) in ['program', 'body_statement', 'then', 'else', 'do_block', 'block_body']`
- `occupies_lines(@stmt)`

**Question:** Remove this breakpoint?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: binding.pry**

```diff
--- a/example.rb
+++ b/example.rb
@@ -1,4 +1,3 @@
 def total(items)
-  binding.pry
   items.sum
 end
```

**Example: byebug without the gem crashes the program**

```diff
--- a/example.rb
+++ b/example.rb
@@ -1,5 +1,4 @@
 def total(items)
-  byebug
   items.sum
 end
 puts total([1, 2, 3])
```

**Counterexample: 1** (no finding)

```ruby
debugger_enabled = false
puts debugger_enabled
```


## fixed-wait

**Fixed wait** · `ruby-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: poll it with a timeout, wait for the process or the event.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported. Waits inside blocks (`loop do`, `.each do`) are treated as loops.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Shape**

```scheme
(call
  method: (identifier) @method
  arguments: (argument_list . [(integer) (float)] @_delay .)
  (#eq? @method "sleep")) @stmt
```

**Facts that must hold**

- `file_purpose() not in ['example', 'docs', 'benchmark']`
- `empty(field(@stmt, 'receiver'))`
- `kind(parent(@stmt)) in ['program', 'body_statement', 'begin_block', 'then', 'else']`
- `!inside(@stmt, 'while', 'until', 'for', 'do_block', 'block')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.rb
+++ b/example.rb
@@ -1,3 +1,2 @@
 puts "start"
-sleep 0.01
 puts "done"
```

**Counterexample: a polling loop** (no finding)

```ruby
until File.exist?("ready")
  sleep 0.1
end
```

