# c-family 0.1.0

> Corrections and questions for C and C++.

**Languages:** c+cpp · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`assignment-in-condition`](#assignment-in-condition) Assignment of a constant in a condition | contradiction | asks, corrects on yes (corrected) | 2 ex. · 5 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 2 ex. · 3 counter |

## assignment-in-condition

**Assignment of a constant in a condition** · `c-family/assignment-in-condition`

category: contradiction · action: asks, corrects on yes (corrected) · CWE-481

`if (retries = 3)` stores 3 in `retries` and then tests 3, which is always
true: the branch always runs and the variable is overwritten. `==` was
almost certainly meant, but an assignment in a condition can be deliberate,
so the engine asks.

**Shape**

```scheme
(if_statement condition: (_) @condition)
```

**Facts that must hold**

- `kind(@assign) == 'assignment_expression' && text(field(@assign, 'operator')) == '='`
- `kind(field(@assign, 'left')) == 'identifier'`
- `kind(field(@assign, 'right')) in ['number_literal', 'char_literal', 'true', 'false']`

**Question:** Did you mean `${variable} == ${value}`?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.c
+++ b/example.c
@@ -2,6 +2,6 @@
 
 int main(void) {
     int retries = 0;
-    if (retries = 3) puts("giving up"); else puts("retrying");
+    if (retries == 3) puts("giving up"); else puts("retrying");
     return 0;
 }
```

**Example: C++**

```diff
--- a/example.c
+++ b/example.c
@@ -2,5 +2,5 @@
 
 int main() {
     int mode = 1;
-    if (mode = 2) std::puts("two"); else std::puts("one");
+    if (mode == 2) std::puts("two"); else std::puts("one");
 }
```

**Counterexample: assigning a computed value** (no finding)

```c
#include <stdio.h>
int main(void) {
    int c;
    if ((c = getchar()) != EOF) putchar(c);
    return 0;
}
```

**Counterexample: a comparison** (no finding)

```c
int is_zero(int x) {
    if (x == 0) return 1;
    return 0;
}
```

**Counterexample: a compound assignment is not a mistyped comparison** (no finding)

```c
int next(int x) {
    if (x += 1) return x;
    return 0;
}
```

**Counterexample: assignment through a pointer is left to a person** (no finding)

```c
void clear(int *p) {
    if (*p = 0) return;
}
```

**Counterexample: assigning a call's result and testing it is an idiom** (no finding)

```c
int read_byte(void);
int first(void) {
    int b;
    if (b = read_byte()) return b;
    return -1;
}
```


## fixed-wait

**Fixed wait** · `c-family/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: join the thread, wait on the channel or future, poll with a timeout.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Shape**

```scheme
(expression_statement
  (call_expression function: (identifier) @function) @call
  (#any-of? @function "sleep" "usleep" "nanosleep" "Sleep")) @stmt

(expression_statement
  (call_expression function: (_) @function) @call) @stmt
```

**Facts that must hold**

- `file_purpose() not in ['example', 'docs', 'benchmark']`
- `text(@function) in ['sleep', 'usleep', 'nanosleep', 'Sleep'] || matches(text(@function), '^(std::)?this_thread::sleep_(for|until)$')`
- `!inside(@stmt, 'for_statement', 'while_statement', 'do_statement', 'for_range_loop', 'lambda_expression')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.c
+++ b/example.c
@@ -3,7 +3,6 @@
 
 int main(void) {
     puts("start");
-    usleep(1000);
     puts("done");
     return 0;
 }
```

**Example: C++ sleep_for** (answer: remove)

```diff
--- a/example.c
+++ b/example.c
@@ -4,6 +4,5 @@
 
 int main() {
     std::cout << "start\n";
-    std::this_thread::sleep_for(std::chrono::milliseconds(1));
     std::cout << "done\n";
 }
```

**Counterexample: a polling loop** (no finding)

```c
#include <unistd.h>
int ready(void);
void wait_ready(void) {
    while (!ready()) {
        usleep(1000);
    }
}
```

**Counterexample: the same wait in an example: pausing there is usually the point** (no finding)

```c
#include <stdio.h>
#include <unistd.h>

int main(void) {
    puts("start");
    usleep(1000);
    puts("done");
    return 0;
}
```

**Counterexample: a wait that shares its line** (no finding)

```c
#include <unistd.h>
void flush_all(void);
void settle(void) { flush_all(); sleep(1); }
```

