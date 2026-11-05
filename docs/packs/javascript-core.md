# javascript-core 0.1.0

> Provable corrections for JavaScript and TypeScript, and questions where intent decides.

JavaScript has its own traps: declarations hoist, `let` and `const` have a
temporal dead zone, NaN is unequal to itself, and debugging leftovers ship.
Every example is executed with Node (TypeScript with type stripping).

**Languages:** javascript+typescript+tsx · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`assignment-in-condition`](#assignment-in-condition) Assignment of a constant in a condition | contradiction | asks, corrects on yes (corrected) | 1 ex. · 1 counter |
| [`console-log`](#console-log) console.log left in the code | waste | asks, then corrects as chosen (corrected) | 1 ex. · 5 counter |
| [`debugger-statement`](#debugger-statement) debugger statement | waste | corrects (preserved) | 1 ex. · 2 counter |
| [`duplicate-object-key`](#duplicate-object-key) Object key given twice | contradiction | explains | 1 ex. · 3 counter |
| [`empty-catch`](#empty-catch) Empty catch block | hazard | explains | 1 ex. · 2 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 2 ex. · 4 counter |
| [`nan-comparison`](#nan-comparison) Comparison with NaN | contradiction | corrects (corrected) | 1 ex. · 4 counter |
| [`return-bool-if`](#return-bool-if) if/else that returns true or false | simplification | corrects (preserved) | 1 ex. · 3 counter |
| [`unreachable-code`](#unreachable-code) Unreachable code | waste | corrects (preserved) | 1 ex. · 4 counter |

## assignment-in-condition

**Assignment of a constant in a condition** · `javascript-core/assignment-in-condition`

category: contradiction · action: asks, corrects on yes (corrected) · CWE-481

`if (mode = "prod")` assigns "prod" to `mode` and then tests the string,
which is always truthy: the branch always runs, and `mode` is overwritten.
Almost always `===` was meant, but an assignment in a condition can be
deliberate, so the engine asks.

**Shape**

```scheme
(if_statement
  condition: (parenthesized_expression
    (assignment_expression
      left: (identifier) @variable
      right: [(number) (string) (true) (false) (null)] @value) @assign))
```

**Question:** Did you mean `${variable} === ${value}`?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.mjs
+++ b/example.mjs
@@ -1,5 +1,5 @@
 let mode = "test";
-if (mode = "prod") {
+if (mode === "prod") {
   console.log("deploying");
 } else {
   console.log("dry run");
```

**Counterexample: assigning a computed value is a common idiom** (no finding)

```javascript
let match;
if ((match = /a/.exec("abc"))) {
  console.log(match.index);
}
```


## console-log

**console.log left in the code** · `javascript-core/console-log`

category: waste · action: asks, then corrects as chosen (corrected) · CWE-489

`console.log` and `console.debug` calls are how most debugging starts, and
many are left behind. Some are the program's real output (a command-line
tool printing results), so the engine asks rather than assumes. keep records
it as intentional; remove deletes it.

Only library code is asked about: console output in tests, examples, scripts, documentation and benchmarks is usually the point of the code (see `file_purpose()`).

**Shape**

```scheme
(expression_statement
  (call_expression
    function: (member_expression object: (identifier) @object property: (property_identifier) @method)) @call
  (#eq? @object "console")
  (#any-of? @method "log" "debug")) @stmt
```

**Facts that must hold**

- `file_purpose() == 'library'`
- `occupies_lines(@stmt)`
- `!js.defines('console')`

**Question:** Keep this console output?

- **keep**: it is real output of the program; record it as intentional
- **remove**: it is leftover debugging; delete it

**Example: 1** (answer: remove)

```diff
--- a/example.mjs
+++ b/example.mjs
@@ -1,5 +1,4 @@
 function add(a, b) {
-  console.log("add called with", a, b);
   return a + b;
 }
 process.stdout.write(String(add(2, 3)) + "\n");
```

**Counterexample: warnings and errors are not debugging leftovers** (no finding)

```javascript
console.error("failed");
console.warn("careful");
```

**Counterexample: a local console is the program's own logger** (no finding)

```javascript
const console = { log: (...xs) => process.stdout.write(xs.join(" ") + "\\n") };
console.log("report ready");
```

**Counterexample: output is the point of an example** (no finding)

```javascript
const port = 3000;
console.log("listening on", port);
```

**Counterexample: and of a script** (no finding)

```javascript
console.log("built");
```

**Counterexample: the only statement of an if: deleting its line would delete the if** (no finding)

```javascript
export function load(config) {
  if (config.verbose) console.log("loading", config.name);
  return config;
}
```


## debugger-statement

**debugger statement** · `javascript-core/debugger-statement`

category: waste · action: corrects (preserved) · CWE-489

A `debugger` statement does nothing unless a debugger is attached, in which
case it stops the program. It is always a leftover from a debugging session:
in production it is dead weight, and for anyone with developer tools open it
halts the page.

**Shape**

```scheme
(debugger_statement) @stmt
```

**Facts that must hold**

- `occupies_lines(@stmt)`

**Example: 1**

```diff
--- a/example.mjs
+++ b/example.mjs
@@ -1,5 +1,4 @@
 function total(items) {
-  debugger;
   return items.reduce((a, b) => a + b, 0);
 }
 console.log(total([1, 2, 3]));
```

**Counterexample: 1** (no finding)

```javascript
const debuggerEnabled = false;
console.log(debuggerEnabled);
```

**Counterexample: the only statement of an if: deleting its line would drop the condition's side effect** (no finding)

```javascript
function check(items) {
  if (validate(items)) debugger;
  return items;
}
function validate(items) { return items.length > 0; }
```


## duplicate-object-key

**Object key given twice** · `javascript-core/duplicate-object-key`

category: contradiction · action: explains · CWE-462

In an object literal a repeated key keeps only its last value. One of the
keys is a typo, or one entry is stale; which one is a question about intent,
so nothing is changed.

**Shape**

```scheme
(object (pair key: (_) @second) @pair)
```

**Facts that must hold**

- `kind(@first) in ['property_identifier', 'string', 'number']`

**Example: 1**

```javascript
const limits = {
  retries: 3,
  timeout: 1000,
  retries: 5,
};
```

**Counterexample: 1** (no finding)

```javascript
const limits = { retries: 3, timeout: 1000 };
```

**Counterexample: distinct keys** (no finding)

```javascript
const limits = { min: 1, max: 10, "default": 5 };
```

**Counterexample: computed keys are evaluated each time and may differ** (no finding)

```javascript
let n = 0;
const next = () => `id${n++}`;
const ids = { [next()]: "a", [next()]: "b" };
console.log(Object.keys(ids));
```


## empty-catch

**Empty catch block** · `javascript-core/empty-catch`

category: hazard · action: explains · CWE-390

An empty `catch` hides every error raised in its `try`, including the ones
nobody anticipated. When ignoring is right, a comment saying why makes it a
decision rather than an accident (and the rule then stays quiet). Otherwise
the error should be handled, logged or rethrown. Which one is a design
question, so nothing is changed.

**Shape**

```scheme
(catch_clause body: (statement_block) @body) @clause
```

**Facts that must hold**

- `count(children(@body)) == 0`
- `!has_comment(@body)`

**Example: 1**

```javascript
try {
  JSON.parse(input);
} catch (e) {}
```

**Counterexample: a comment explains why** (no finding)

```javascript
try {
  JSON.parse(input);
} catch (e) {
  // invalid input is reported by the caller
}
```

**Counterexample: a catch that handles the error** (no finding)

```javascript
function parse(text) {
  try {
    return JSON.parse(text);
  } catch (err) {
    console.error("not JSON:", err.message);
    return null;
  }
}
```


## fixed-wait

**Fixed wait** · `javascript-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

`await new Promise(r => setTimeout(r, ms))`, `page.waitForTimeout(ms)` and
`cy.wait(ms)` are guesses about timing: too short and tests are flaky, too
long and every run is slow. The safe form waits for the thing itself: await
the promise that produces it, or use the test framework's waiting assertions
(`await expect(locator).toBeVisible()`, `cy.get(...).should(...)`).

Some waits are deliberate (rate limits, back-off, animation pacing), so the
engine asks. keep records the wait as intentional; remove deletes it. Waits
inside loops are polling intervals and are not reported.

Examples, documentation and benchmarks are left alone (see `file_purpose()`): a pause there is part of the demonstration.

**Shape**

```scheme
(expression_statement
  (await_expression
    (new_expression
      constructor: (identifier) @promise
      arguments: (arguments
        (arrow_function
          parameter: (identifier) @resolve
          body: (call_expression
            function: (identifier) @timer
            arguments: (arguments . (identifier) @callback . (_) @_delay .)))))) @call
  (#eq? @promise "Promise")
  (#eq? @timer "setTimeout")
  (#eq? @resolve @callback)) @stmt

(expression_statement
  (await_expression
    (new_expression
      constructor: (identifier) @promise
      arguments: (arguments
        (arrow_function
          parameters: (formal_parameters . (_) @resolve .)
          body: (call_expression
            function: (identifier) @timer
            arguments: (arguments . (identifier) @callback . (_) @_delay .)))))) @call
  (#eq? @promise "Promise")
  (#eq? @timer "setTimeout")
  (#eq? @resolve @callback)) @stmt

(expression_statement
  (await_expression
    (call_expression
      function: (member_expression property: (property_identifier) @method)
      arguments: (arguments . (number) @_delay .))) @call
  (#any-of? @method "waitForTimeout" "pause")) @stmt

(expression_statement
  (call_expression
    function: (member_expression object: (identifier) @object property: (property_identifier) @method)
    arguments: (arguments . (number) @_delay .)) @call
  (#eq? @object "cy")
  (#eq? @method "wait")) @stmt

(expression_statement
  (await_expression
    (call_expression
      function: (identifier) @helper
      arguments: (arguments . (number) @_delay .))) @call
  (#any-of? @helper "sleep" "delay" "wait" "pause")) @stmt
```

**Facts that must hold**

- `file_purpose() not in ['example', 'docs', 'benchmark']`
- `!inside(@stmt, 'for_statement', 'for_in_statement', 'while_statement', 'do_statement')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: a setTimeout promise** (answer: remove)

```diff
--- a/example.mjs
+++ b/example.mjs
@@ -1,6 +1,5 @@
 async function checkout(cart) {
   console.log("charging", cart.total);
-  await new Promise((resolve) => setTimeout(resolve, 20));
   console.log("charged");
 }
 
```

**Example: in TypeScript, kept** (answer: keep)

```diff
async function poll(): Promise<void> {
  await new Promise(r => setTimeout(r, 10));
  console.log("polled");
}
await poll();
(unchanged)
```

**Counterexample: a polling loop** (no finding)

```javascript
async function waitFor(ready) {
  while (!ready()) {
    await new Promise((r) => setTimeout(r, 50));
  }
}
```

**Counterexample: a deferred callback is not a wait** (no finding)

```javascript
setTimeout(() => console.log("later"), 1000);
```

**Counterexample: the same wait in an example: pausing there is usually the point** (no finding)

```javascript
async function checkout(cart) {
  console.log("charging", cart.total);
  await new Promise((resolve) => setTimeout(resolve, 20));
  console.log("charged");
}

await checkout({ total: 42 });
```

**Counterexample: a wait that shares its line** (no finding)

```javascript
async function settle() { flush(); await sleep(100); }
```


## nan-comparison

**Comparison with NaN** · `javascript-core/nan-comparison`

category: contradiction · action: corrects (corrected) · CWE-1077

`x === NaN` is false for every `x`, NaN included, and `x !== NaN` is always
true: NaN compares unequal to everything. The code clearly means "is x NaN?",
which is `Number.isNaN(x)`. The rule applies only when the file does not
redefine `NaN` or `Number`.

**Shape**

```scheme
(binary_expression
  left: (_) @value
  operator: ["===" "!==" "==" "!="] @op
  right: (identifier) @nan
  (#eq? @nan "NaN")) @cmp

(binary_expression
  left: (identifier) @nan
  operator: ["===" "!==" "==" "!="] @op
  right: (_) @value
  (#eq? @nan "NaN")) @cmp
```

**Facts that must hold**

- `!js.defines('NaN')`
- `!js.defines('Number')`
- `text(@value) != 'NaN'`

**Example: 1**

```diff
--- a/example.mjs
+++ b/example.mjs
@@ -1,3 +1,3 @@
 const values = [1, NaN, 3];
-console.log(values.filter((v) => v === NaN).length);
-console.log(values.filter((v) => NaN != v).length);
+console.log(values.filter((v) => Number.isNaN(v)).length);
+console.log(values.filter((v) => !Number.isNaN(v)).length);
```

**Counterexample: 1** (no finding)

```javascript
const v = 1;
console.log(Number.isNaN(v));
```

**Counterexample: a local NaN is an ordinary value** (no finding)

```javascript
const NaN = 0;
console.log(1 === NaN);
```

**Counterexample: a local Number has no isNaN** (no finding)

```javascript
const Number = { parse: (s) => +s };
console.log(Number.parse("1") === NaN);
```

**Counterexample: NaN against itself: a demonstration of IEEE 754, not a mistaken check** (no finding)

```javascript
console.log(NaN === NaN);
```


## return-bool-if

**if/else that returns true or false** · `javascript-core/return-bool-if`

category: simplification · action: corrects (preserved)

`if (c) { return true; } else { return false; }` returns the truth value of
`c`, which is what `Boolean(c)` computes. JavaScript has no operator
overloading, so comparisons (`===`, `<`, `instanceof`, `in`...) and `!`
always produce a boolean already, and then `return c;` is exact. The rule
applies only when the file does not redefine `Boolean`.

**Shape**

```scheme
(if_statement
  condition: (parenthesized_expression (_) @cond)
  consequence: [(statement_block . (return_statement (true)) .) (return_statement (true))]
  alternative: (else_clause [(statement_block . (return_statement (false)) .) (return_statement (false))])) @whole

(statement_block
  (if_statement
    condition: (parenthesized_expression (_) @cond)
    consequence: [(statement_block . (return_statement (true)) .) (return_statement (true))]
    !alternative) @whole
  .
  (return_statement (false)) @whole)
```

**Facts that must hold**

- `occupies_lines(@whole)`
- `!js.defines('Boolean')`

**Example: 1**

```diff
--- a/example.mjs
+++ b/example.mjs
@@ -1,14 +1,9 @@
 function isAdmin(user, admins) {
-  if (admins.includes(user) === true) {
-    return true;
-  } else {
-    return false;
-  }
+  return admins.includes(user) === true;
 }
 
 function hasItems(cart) {
-  if (cart.items.length) return true;
-  return false;
+  return Boolean(cart.items.length);
 }
 
 console.log(isAdmin("ada", ["ada"]), isAdmin("bob", ["ada"]));
```

**Counterexample: more work before returning** (no finding)

```javascript
function check(n) {
  if (n) {
    console.log("yes");
    return true;
  }
  return false;
}
```

**Counterexample: a one-line function is left as written** (no finding)

```javascript
function big(x) { if (x > 1) { return true; } return false; }
```

**Counterexample: Boolean is redefined here, so Boolean(...) would not be the builtin** (no finding)

```javascript
const Boolean = (x) => (x ? "yes" : "no");
function has(list) {
  if (list.length) {
    return true;
  }
  return false;
}
```


## unreachable-code

**Unreachable code** · `javascript-core/unreachable-code`

category: waste · action: corrects (preserved) · CWE-561

Statements after `return`, `throw`, `break` or `continue` in the same block
never execute.

Deleting them is usually safe, but JavaScript has two exceptions the rule
checks. Function declarations and `var` are hoisted: they exist from the top
of their scope, even when written after a return, so code above may use
them. And `let`, `const` and `class` create a temporal dead zone: a dead
`let x` still makes every earlier `x` in the block refer to it (and throw).
If the unreachable statements declare anything at all, nothing is reported.

**Shape**

```scheme
(statement_block [(return_statement) (throw_statement) (break_statement) (continue_statement)] @jump)
```

**Facts that must hold**

- `occupies_lines(@dead)`
- `!contains(@dead, 'function_declaration', 'generator_function_declaration', 'class_declaration', 'variable_declaration', 'lexical_declaration', 'abstract_class_declaration', 'enum_declaration')`

**Example: 1**

```diff
--- a/example.mjs
+++ b/example.mjs
@@ -1,9 +1,7 @@
 function price(n) {
   if (n < 0) {
     throw new Error("negative");
-    console.log("unreachable");
   }
   return n * 2;
-  console.log("also unreachable");
 }
 console.log(price(21));
```

**Counterexample: a hoisted function declaration** (no finding)

```javascript
function f() {
  return helper();
  function helper() {
    return 1;
  }
}
console.log(f());
```

**Counterexample: a dead let still has a temporal dead zone** (no finding)

```javascript
const x = "outer";
function show() {
  try {
    console.log(typeof x);
  } catch (e) {
    console.log("x is in its dead zone");
  }
  return;
  let x = 1;
}
show();
```

**Counterexample: jumps that end their block leave nothing behind them** (no finding)

```javascript
function sign(x) {
  if (x < 0) {
    return -1;
  }
  for (const y of [x]) {
    if (y === 0) {
      break;
    }
  }
  return 1;
}
```

**Counterexample: dead code that shares the jump's line: deleting the line would delete the jump** (no finding)

```javascript
function first(list) {
  if (list.length) { return list[0]; cleanup(); }
  return undefined;
}
```

