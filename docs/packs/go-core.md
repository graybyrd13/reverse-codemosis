# go-core 0.1.0

> Provable corrections for Go, and questions where intent decides.

**Languages:** go · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`bool-literal-comparison`](#bool-literal-comparison) Comparison with true or false | simplification | corrects (preserved) | 1 ex. · 1 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 2 counter |
| [`needless-return`](#needless-return) Bare return at the end of a function | simplification | corrects (preserved) | 1 ex. · 1 counter |

## bool-literal-comparison

**Comparison with true or false** · `go-core/bool-literal-comparison`

category: simplification · action: corrects (preserved)

Go only lets a boolean be compared with `true` or `false`, so `x == true` is
exactly `x`, `x == false` and `x != true` are exactly `!x`, and `x != false`
is exactly `x`. The shorter form says the same thing directly.

**Shape**

```scheme
(binary_expression
  left: (_) @value
  operator: ["==" "!="] @op
  right: [(true) (false)] @literal) @cmp

(binary_expression
  left: [(true) (false)] @literal
  operator: ["==" "!="] @op
  right: (_) @value) @cmp
```

**Facts that must hold**

- `kind(@value) not in ['true', 'false']`

**Example: 1**

```diff
--- a/example.go
+++ b/example.go
@@ -4,13 +4,13 @@
 
 func main() {
 	ready := len("ab") > 1
-	if ready == true {
+	if ready {
 		fmt.Println("ready")
 	}
-	if ready != true {
+	if !ready {
 		fmt.Println("not ready")
 	}
-	if false == (len("a") > 3) {
+	if !(len("a") > 3) {
 		fmt.Println("short")
 	}
 }
```

**Counterexample: 1** (no finding)

```go
package main

func f(a, b int) bool {
	return a == b
}
```


## fixed-wait

**Fixed wait** · `go-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: join the thread, wait on the channel or future, poll with a timeout.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

Go refuses to compile an unused import, so the wait is only reported when
the file uses the time package elsewhere too.

**Shape**

```scheme
(expression_statement
  (call_expression
    function: (selector_expression operand: (identifier) @package field: (field_identifier) @function)) @call
  (#eq? @package "time")
  (#eq? @function "Sleep")) @stmt
```

**Facts that must hold**

- `!inside(@stmt, 'for_statement')`
- `occupies_lines(@stmt)`
- `file_matches('\\btime\\.') > count_matches(text(@stmt), '\\btime\\.')`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.go
+++ b/example.go
@@ -8,6 +8,5 @@
 func main() {
 	start := time.Now()
 	fmt.Println("started")
-	time.Sleep(5 * time.Millisecond)
 	fmt.Println("finished", start.Before(time.Now().Add(time.Hour)))
 }
```

**Counterexample: a polling loop** (no finding)

```go
package main

import "time"

func wait(ready func() bool) {
	for !ready() {
		time.Sleep(10 * time.Millisecond)
	}
}
```

**Counterexample: removing it would leave the time import unused** (no finding)

```go
package main

import "time"

func main() {
	time.Sleep(10)
}
```


## needless-return

**Bare return at the end of a function** · `go-core/needless-return`

category: simplification · action: corrects (preserved)

A function with no results returns when it reaches its end. A bare `return`
as its very last statement changes nothing: there are no result parameters
for it to set, and deferred calls run the same either way. Deleting it is
the form `go vet` and most Go style guides expect. The rule only applies to
functions without results, where a bare return cannot carry a value.

**Shape**

```scheme
(function_declaration
  !result
  body: (block (statement_list (return_statement) @ret .)))

(method_declaration
  !result
  body: (block (statement_list (return_statement) @ret .)))
```

**Facts that must hold**

- `count(children(@ret)) == 0`
- `occupies_lines(@ret)`

**Example: 1**

```diff
--- a/example.go
+++ b/example.go
@@ -4,7 +4,6 @@
 
 func greet() {
 	fmt.Println("hi")
-	return
 }
 
 func main() {
```

**Counterexample: an early return** (no finding)

```go
package main

func f(n int) {
	if n < 0 {
		return
	}
	println(n)
}
```

