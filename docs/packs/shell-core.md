# shell-core 0.1.0

> Questions for shell scripts: fixed waits and unquoted deletions.

**Languages:** bash · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |
| [`unquoted-rm`](#unquoted-rm) Unquoted variable in rm | hazard | asks, corrects on yes (corrected) | 1 ex. · 1 counter |

## fixed-wait

**Fixed wait** · `shell-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: poll it with a timeout, wait for the process or the event.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

In scripts the usual safe form is a loop that checks the condition (the port
answers, the file exists, the process is up) with a time limit.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Shape**

```scheme
(command
  name: (command_name (word) @command)
  argument: [(number) (word)] @_delay
  (#eq? @command "sleep")) @stmt
```

**Facts that must hold**

- `file_purpose() not in ['example', 'docs', 'benchmark']`
- `count(field(@stmt, 'argument')) == 1`
- `kind(parent(@stmt)) == 'program' || (kind(parent(@stmt)) == 'compound_statement' && count(children(parent(@stmt))) > 1)`
- `!inside(@stmt, 'for_statement', 'while_statement', 'c_style_for_statement')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.sh
+++ b/example.sh
@@ -1,3 +1,2 @@
 echo "starting the server"
-sleep 0.01
 echo "server started"
```

**Counterexample: a polling loop** (no finding)

```bash
until curl -s localhost:8080 >/dev/null; do
  sleep 1
done
```


## unquoted-rm

**Unquoted variable in rm** · `shell-core/unquoted-rm`

category: hazard · action: asks, corrects on yes (corrected)

An unquoted variable is split on spaces and expanded as a glob. In `rm`,
that turns "my dir/build" into two paths, and an empty or unset variable
turns `rm -rf $DIR/build` into `rm -rf /build`. Quoting the variable keeps
the path exactly as it was set. Some scripts rely on splitting a list of
paths on purpose, so the engine asks.

**Shape**

```scheme
(command
  name: (command_name (word) @command)
  argument: (concatenation (simple_expansion) @variable)
  (#eq? @command "rm")) @stmt

(command
  name: (command_name (word) @command)
  argument: (simple_expansion) @variable
  (#eq? @command "rm")) @stmt
```

**Question:** Quote ${variable} so that spaces or an empty value cannot change what is deleted?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.sh
+++ b/example.sh
@@ -1,4 +1,4 @@
 DIR="my dir"
 mkdir -p "$DIR/build" && touch "$DIR/build/output"
-rm -rf $DIR/build
+rm -rf "$DIR"/build
 ls "$DIR" | wc -l | tr -d ' '
```

**Counterexample: 1** (no finding)

```bash
rm -rf "$DIR/build"
```

