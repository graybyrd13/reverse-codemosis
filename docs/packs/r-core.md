# r-core 0.1.0

> Corrections and questions for R: comparisons with NA, browser() left in code.

**Languages:** r · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`browser-call`](#browser-call) browser() left in code | waste | asks, then corrects as chosen (preserved) | 1 ex. · 3 counter |
| [`equals-na`](#equals-na) Comparison with NA | contradiction | asks, corrects on yes (corrected) | 2 ex. · 2 counter |

## browser-call

**browser() left in code** · `r-core/browser-call`

category: waste · action: asks, then corrects as chosen (preserved) · tags: debugging

`browser()` pauses an interactive session in the debugger; it is how R code is debugged, and it is easy to
leave behind. The engine asks: keep records it as intentional; remove deletes the line.

**Shape**

```scheme
(call function: (identifier) @function arguments: (arguments) (#eq? @function "browser")) @call
```

**Facts that must hold**

- `kind(parent(@call)) in ['braced_expression', 'program']`
- `occupies_lines(@call)`
- `!r.defines('browser')`

**Question:** Keep this browser() call?

- **keep**: it is deliberate; record it as intentional
- **remove**: it is leftover debugging; delete it

**Example: 1** (answer: remove)

```diff
--- a/example.R
+++ b/example.R
@@ -1,4 +1,3 @@
 total <- function(x) {
-  browser()
   sum(x)
 }
```

**Counterexample: the body of an if: deleting the line would change what the if guards** (no finding)

```text
total <- function(x, debug) {
  if (debug)
    browser()
  sum(x)
}
```

**Counterexample: a call that shares its line** (no finding)

```text
total <- function(x) {
  browser(); sum(x)
}
```

**Counterexample: browser is defined here** (no finding)

```text
browser <- function() message("open the report")
show <- function() {
  browser()
  invisible(NULL)
}
```


## equals-na

**Comparison with NA** · `r-core/equals-na`

category: contradiction · action: asks, corrects on yes (corrected) · tags: missing-values

In R, any comparison with `NA` is itself `NA`: `x == NA` is `NA` whatever `x` is, so an `if` on it fails
with "missing value where TRUE/FALSE needed" and a filter on it keeps nothing. `is.na(x)` is the test for
a missing value.

A class can redefine `==` (through `Ops`), so the engine asks before correcting.

**Shape**

```scheme
(binary_operator lhs: (_) @value operator: ["==" "!="] @op rhs: (na) @_na) @cmp
(binary_operator lhs: (na) @_na operator: ["==" "!="] @op rhs: (_) @value) @cmp
```

**Facts that must hold**

- `kind(@value) != 'na'`
- `!r.defines('is.na')`

**Question:** Did you mean `${check}`?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.R
+++ b/example.R
@@ -1,4 +1,4 @@
 clean <- function(x) {
-  if (x == NA) 0 else x
+  if (is.na(x)) 0 else x
 }
 print(clean(NA))
```

**Example: not equal**

```diff
--- a/example.R
+++ b/example.R
@@ -1 +1 @@
-present <- scores[NA != scores]
+present <- scores[!is.na(scores)]
```

**Counterexample: NA against NA** (no finding)

```text
both <- NA == NA
```

**Counterexample: is.na is redefined here** (no finding)

```text
is.na <- function(x) FALSE
missing <- x == NA
```

