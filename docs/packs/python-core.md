# python-core 0.1.0

> Provable corrections for everyday Python 3.

The corrections in this pack are the ones a careful reviewer makes by reflex:
missing standard-library imports, identity tests against literals, vacuous
assertions, unreachable code, unused imports, and simpler forms that are
provably equal. Where a correction depends on intent (mutable defaults, bare
except, a string that looks like it meant to be an f-string), the pack asks.

**Languages:** python · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`bare-except`](#bare-except) Bare except clause | hazard | asks, corrects on yes (corrected) | 1 ex. · 2 counter |
| [`duplicate-dict-key`](#duplicate-dict-key) Dictionary key given twice | contradiction | explains | 1 ex. · 2 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 3 ex. · 3 counter |
| [`fstring-without-placeholders`](#fstring-without-placeholders) f-string without placeholders | waste | corrects (preserved) | 1 ex. · 1 counter |
| [`is-literal`](#is-literal) Identity comparison with a literal | contradiction | corrects (corrected) | 2 ex. · 3 counter |
| [`missing-f-prefix`](#missing-f-prefix) String with placeholders but no f prefix | contradiction | asks, corrects on yes (corrected) | 1 ex. · 4 counter |
| [`missing-module-import`](#missing-module-import) Module used without being imported | broken | corrects (repaired) | 2 ex. · 3 counter |
| [`missing-name-import`](#missing-name-import) Standard-library name used without being imported | broken | corrects (repaired) | 2 ex. · 2 counter |
| [`mutable-default`](#mutable-default) Mutable default argument | hazard | asks, corrects on yes (corrected) | 2 ex. · 2 counter |
| [`none-equality`](#none-equality) Equality comparison with None | hazard | asks, corrects on yes (preserved) | 1 ex. · 2 counter |
| [`redundant-list-around-sorted`](#redundant-list-around-sorted) list() around sorted() | waste | corrects (preserved) | 1 ex. · 3 counter |
| [`redundant-pass`](#redundant-pass) Redundant pass | waste | corrects (preserved) | 1 ex. · 1 counter |
| [`return-bool-if-negated`](#return-bool-if-negated) if/else that returns False or True | simplification | corrects (preserved) | 1 ex. · 1 counter |
| [`return-bool-if`](#return-bool-if) if/else that returns True or False | simplification | corrects (preserved) | 1 ex. · 2 counter |
| [`undefined-name`](#undefined-name) Name that is never defined | broken | explains | 2 ex. · 2 counter |
| [`unreachable-code`](#unreachable-code) Unreachable code | waste | corrects (preserved) | 2 ex. · 3 counter |
| [`unused-import`](#unused-import) Unused import | waste | corrects (preserved) | 1 ex. · 5 counter |
| [`vacuous-assert`](#vacuous-assert) Assertion that can never fail | contradiction | corrects (corrected) | 1 ex. · 2 counter |

## bare-except

**Bare except clause** · `python-core/bare-except`

category: hazard · action: asks, corrects on yes (corrected) · CWE-396

`except:` with no exception type catches everything that derives from
BaseException, including KeyboardInterrupt (Ctrl-C), SystemExit
(sys.exit()) and GeneratorExit. A handler written to recover from ordinary
errors then also swallows the user's request to stop.

`except Exception:` catches every ordinary error and lets those three pass.
Whether this handler should let them pass is a decision about intent, so the
correction is applied only after you answer yes. Handlers that re-raise are
not reported: catching everything and re-raising is a legitimate cleanup
pattern.

**Shape**

```scheme
(except_clause "except" @keyword !value) @clause
```

**Facts that must hold**

- `!contains(@clause, 'raise_statement')`

**Question:** Should this handler let KeyboardInterrupt (Ctrl-C) and SystemExit (sys.exit()) pass through, catching only ordinary errors?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -4,7 +4,7 @@
 def shutdown():
     try:
         sys.exit("stopping")
-    except:
+    except Exception:
         print("ignored the request to stop")
 
 
```

**Counterexample: a typed handler** (no finding)

```python
try:
    pass
except Exception:
    pass
```

**Counterexample: cleanup that re-raises** (no finding)

```python
def run(job):
    try:
        job()
    except:
        print("cleaning up")
        raise
```


## duplicate-dict-key

**Dictionary key given twice** · `python-core/duplicate-dict-key`

category: contradiction · action: explains · CWE-462

In a dictionary literal, a repeated key keeps only its last value: the
earlier entry is evaluated and then thrown away. Either one of the keys is a
typo for a different key, or one of the entries is stale. Which one is a
question about intent, so nothing is changed: decide which entry is right
and delete or rename the other.

**Shape**

```scheme
(dictionary
  (pair key: (_) @first)
  (pair key: (_) @second)
  (#eq? @first @second))
```

**Facts that must hold**

- `kind(@first) in ['string', 'integer', 'float', 'true', 'false', 'none']`
- `!contains(@first, 'interpolation')`

**Example: 1**

```python
PRICES = {
    "apple": 0.5,
    "pear": 0.75,
    "apple": 0.6,
}
```

**Counterexample: distinct keys** (no finding)

```python
PRICES = {"apple": 0.5, "pear": 0.75}
```

**Counterexample: computed keys may differ at run time** (no finding)

```python
counter = iter(range(10))
d = {next(counter): "a", next(counter): "b"}
```


## fixed-wait

**Fixed wait** · `python-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing. Too short and the code is flaky (the
thing it waited for is not ready yet); too long and it is slow, every run.
AI-written code and quick fixes reach for one whenever something "needs a
moment". The safe form waits for the condition itself: poll it with a
timeout, await the event, join the thread or process.

Some waits are deliberate: rate limits, back-off, pacing output for a person.
Only you know which this is, so the engine asks. Answering keep records the
wait as intentional; remove deletes it. Waits inside loops are polling
intervals and are not reported.

**Shape**

```scheme
(expression_statement
  (call
    function: (attribute object: (identifier) @module attribute: (identifier) @function)
    arguments: (argument_list)) @call
  (#eq? @module "time")
  (#eq? @function "sleep")) @stmt

(expression_statement
  (await
    (call
      function: (attribute object: (identifier) @module attribute: (identifier) @function)
      arguments: (argument_list)) @call)
  (#any-of? @module "asyncio" "trio" "anyio")
  (#eq? @function "sleep")) @stmt
```

**Facts that must hold**

- `!inside(@stmt, 'for_statement', 'while_statement')`
- `occupies_lines(@stmt)`
- `count(children(parent(@stmt))) > 1`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: remove** (answer: remove)

```diff
--- a/example.py
+++ b/example.py
@@ -3,7 +3,6 @@
 
 def fetch():
     print("request sent")
-    time.sleep(0.01)
     print("response read")
 
 
```

**Example: keep** (answer: keep)

```diff
import time

print("pacing")
time.sleep(0.01)
print("done")
(unchanged)
```

**Example: an awaited asyncio sleep** (answer: remove)

```diff
--- a/example.py
+++ b/example.py
@@ -3,7 +3,6 @@
 
 async def main():
     print("connecting")
-    await asyncio.sleep(0.01)
     print("connected")
 
 
```

**Counterexample: a polling interval inside a loop** (no finding)

```python
import time


def wait_until(ready):
    while not ready():
        time.sleep(0.1)
```

**Counterexample: the only statement of a block** (no finding)

```python
import time


def pause():
    time.sleep(1)
```

**Counterexample: a wait that paces a polling loop** (no finding)

```python
import time

for attempt in range(3):
    time.sleep(1)
```


## fstring-without-placeholders

**f-string without placeholders** · `python-core/fstring-without-placeholders`

category: waste · action: corrects (preserved)

An f-string with no `{...}` placeholders is just a string with a misleading
prefix: readers look for the interpolation that is not there. Removing the
`f` gives an identical value, provided escaped braces are unescaped: in an
f-string `{{` means one brace, in a plain string it means two.

**Shape**

```scheme
(string) @s
```

**Facts that must hold**

- `matches(py.string_prefix(@s), 'f')`
- `!contains(@s, 'interpolation')`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,3 +1,3 @@
-print(f"Hello, world")
-print(f'{{literal}} braces')
-print(rf"C:\\temp")
+print("Hello, world")
+print('{literal} braces')
+print(r"C:\\temp")
```

**Counterexample: an f-string with a placeholder** (no finding)

```python
name = "Ada"
print(f"Hello, {name}")
```


## is-literal

**Identity comparison with a literal** · `python-core/is-literal`

category: contradiction · action: corrects (corrected) · CWE-595

`is` compares object identity. Whether two equal numbers or strings are the
same object is an accident of the interpreter: CPython caches small integers
and interns some strings, so `x is 5` or `name is "admin"` can appear to work
in a quick test and then fail for a value read from a file, parsed from
input or built by string operations. CPython itself warns about it
(SyntaxWarning: "is" with a literal).

A literal has no identity anyone else can hold, so the only meaning the
comparison can portably have is equality. The correction replaces `is` with
`==` and `is not` with `!=`. `None`, `True`, `False` and `...` are
singletons and are never touched.

**Shape**

```scheme
[
  (comparison_operator
    (_) @_left
    operators: ["is" "is not"] @op
    [(integer) (float) (string) (concatenated_string)] @_literal)
  (comparison_operator
    [(integer) (float) (string) (concatenated_string)] @_literal
    operators: ["is" "is not"] @op
    (_) @_right)
] @cmp
```

**Facts that must hold**

- `field_count(@cmp, 'operators') == 1`

**Example: a string built at run time**

```diff
--- a/example.py
+++ b/example.py
@@ -1,3 +1,3 @@
 status = "".join(["act", "ive"])
-print(status is "active")
-print(status is not "active")
+print(status == "active")
+print(status != "active")
```

**Example: a number parsed from text**

```diff
--- a/example.py
+++ b/example.py
@@ -1,5 +1,5 @@
 count = int("1000")
-if 1000 is count:
+if 1000 == count:
     print("match")
 else:
     print("no match")
```

**Counterexample: singletons are compared by identity** (no finding)

```python
value = None
print(value is None, value is not True)
```

**Counterexample: comparing two names is a real identity test** (no finding)

```python
a = b = []
print(a is b)
```

**Counterexample: a chained comparison is not a single identity test** (no finding)

```python
x = 3
print(x is 3 < 4)
```

**References**

- <https://docs.python.org/3/reference/expressions.html#is-not>


## missing-f-prefix

**String with placeholders but no f prefix** · `python-core/missing-f-prefix`

category: contradiction · action: asks, corrects on yes (corrected)

The string contains `{name}` placeholders, and every name in them is a
variable visible at this point, but the string has no `f` prefix and is not
passed to `.format()` or `%`. Python will output the braces and names
literally.

That looks like a forgotten `f`, but it could also be a template filled in
somewhere else, so the decision is yours. Answering yes adds the `f` prefix.

**Shape**

```scheme
(string) @s
```

**Facts that must hold**

- `py.placeholders_bound(@s)`
- `!py.is_docstring(@s)`
- `kind(parent(@s)) not in ['attribute', 'binary_operator', 'concatenated_string', 'pair', 'subscript']`

**Question:** Should ${s} be an f-string, so its placeholders are filled in from the variables in scope?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,5 +1,5 @@
 def greet(name):
-    return "Hello, {name}!"
+    return f"Hello, {name}!"
 
 
 print(greet("Ada"))
```

**Counterexample: a template filled in by .format** (no finding)

```python
def greet(name):
    return "Hello, {name}!".format(name=name)
```

**Counterexample: placeholder names that are not variables here** (no finding)

```python
TEMPLATE = "Dear {customer}, your order {order_id} has shipped."
```

**Counterexample: escaped braces** (no finding)

```python
name = "x"
pattern = "{{name}} and {name}"
```

**Counterexample: a docstring that shows a placeholder** (no finding)

```python
def greet(name):
    """Return "Hello, {name}"."""
    return "Hello, " + name
```


## missing-module-import

**Module used without being imported** · `python-core/missing-module-import`

category: broken · action: corrects (repaired)

The file reads `name.something`, but nothing in the file defines `name`: no
import, no assignment, no parameter, no function or class of that name, and
the file cannot create names at run time (no star import, exec, eval or
globals()). Python raises NameError the first time the line runs.

The repair is applied only when it is the only possible one: `name` is a
standard-library module, every attribute the file reads from it exists in
that module, and no other file in the project defines a name or a module
called `name`. Then exactly one statement makes the code run: `import name`.

**Shape**

```scheme
(attribute object: (identifier) @name) @_use
```

**Facts that must hold**

- `py.is_reference(@name)`
- `!py.bound(text(@name))`
- `!py.builtin(text(@name))`
- `!py.dynamic_names()`
- `in_table('stdlib_modules', text(@name))`
- `subset(py.attributes(text(@name)), table('stdlib_modules', text(@name)))`
- `!py.defined_in_project(text(@name), text(@name))`

**Example: joins the standard-library imports in alphabetical order**

```diff
--- a/example.py
+++ b/example.py
@@ -1,3 +1,4 @@
+import json
 import os
 
 
```

**Example: one import serves every use**

```diff
--- a/example.py
+++ b/example.py
@@ -1,3 +1,5 @@
+import math
+
 def area(r):
     return math.pi * r * r
 
```

**Counterexample: the name is a parameter** (no finding)

```python
def save(json, data):
    return json.dumps(data)
```

**Counterexample: json.parse is not part of the json module** (no finding)

```python
def load(text):
    return json.parse(text)
```

**Counterexample: a star import may provide the name** (no finding)

```python
from helpers import *

print(json.dumps({}))
```

**References**

- <https://docs.python.org/3/reference/executionmodel.html#resolution-of-names>


## missing-name-import

**Standard-library name used without being imported** · `python-core/missing-name-import`

category: broken · action: corrects (repaired)

The file reads a name that nothing in the file defines, so Python raises
NameError when the line runs (or when the module is imported, if the line is
at module level or in an annotation).

The repair is applied only when exactly one standard-library module exports
that name (the pack's `stdlib_names` table lists only such names: `Path`
from pathlib, `Optional` from typing, `dataclass` from dataclasses...), the
file cannot create names at run time, and no other file in the project
defines the name. Then `from module import name` is the only repair. When the
file already imports other names from that module on one line, the name is
added to that line.

**Shape**

```scheme
(identifier) @name
```

**Facts that must hold**

- `in_table('stdlib_names', text(@name))`
- `py.is_reference(@name)`
- `!py.bound(text(@name))`
- `!py.builtin(text(@name))`
- `!py.dynamic_names()`
- `!py.defined_in_project(text(@name), lookup('stdlib_names', text(@name)))`

**Example: adds a from-import**

```diff
--- a/example.py
+++ b/example.py
@@ -1,4 +1,5 @@
 import json
+from pathlib import Path
 
 
 def config_dir(home):
```

**Example: joins an existing from-import**

```diff
--- a/example.py
+++ b/example.py
@@ -1,4 +1,4 @@
-from typing import List
+from typing import List, Optional
 
 
 def first(items: List[int]) -> Optional[int]:
```

**Counterexample: the name is defined in the file** (no finding)

```python
class Path:
    pass

print(Path())
```

**Counterexample: the name is ambiguous in the standard library** (no finding)

```python
def wait():
    sleep(1)
```


## mutable-default

**Mutable default argument** · `python-core/mutable-default`

category: hazard · action: asks, corrects on yes (corrected)

Default values are evaluated once, when the `def` statement runs, not on
every call. A list, dict or set default is therefore a single object shared
by every call that leaves the argument out: anything one call adds to it is
still there in the next call.

The standard safe form is a `None` default and a fresh object created inside
the function. That form also treats an explicit `None` from a caller as
"use the default", and it stops any deliberate sharing between calls (some
code uses a mutable default as a cache on purpose). Both are decisions about
intent, so the correction is applied only after you answer yes.

**Shape**

```scheme
(function_definition
  name: (identifier) @function
  parameters: (parameters
    [
      (default_parameter name: (identifier) @param value: [(list) (dictionary) (set)] @value)
      (typed_default_parameter name: (identifier) @param value: [(list) (dictionary) (set)] @value)
    ])
  body: (block) @body) @_def
```

**Facts that must hold**

- `exists(@first)`
- `!same_line(@first, @function)`

**Question:** Should every call to `${function}` that leaves out `${param}` get a fresh `${value}`, instead of all such calls sharing one? (An explicit `${param}=None` will then also mean the default.)

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,4 +1,6 @@
-def add_tag(tag, tags=[]):
+def add_tag(tag, tags=None):
+    if tags is None:
+        tags = []
     tags.append(tag)
     return tags
 
```

**Example: after the docstring, with a type annotation**

```diff
--- a/example.py
+++ b/example.py
@@ -1,5 +1,7 @@
-def register(name: str, seen: dict = {}) -> int:
+def register(name: str, seen: dict = None) -> int:
     """Count registrations per name."""
+    if seen is None:
+        seen = {}
     seen[name] = seen.get(name, 0) + 1
     return seen[name]
 
```

**Counterexample: immutable defaults are safe** (no finding)

```python
def f(items=(), name="x", limit=None):
    return items
```

**Counterexample: a one-line function has no room for the guard** (no finding)

```python
def f(items=[]): return items
```


## none-equality

**Equality comparison with None** · `python-core/none-equality`

category: hazard · action: asks, corrects on yes (preserved)

`x == None` calls `x.__eq__(None)`, which any class can redefine. NumPy
arrays answer with an array, pandas objects and ORM columns answer with
expressions, and some classes answer True. `x is None` cannot be redefined:
it is true exactly when `x` is None.

For ordinary objects the two forms always agree, and then `is None` is the
correct one. But if `x` can be one of those special objects, `== None` may be
deliberate (SQLAlchemy filters are written that way). Only you know what
`x` can hold, so the correction is applied only after you answer yes.

**Shape**

```scheme
[
  (comparison_operator (_) @left operators: ["==" "!="] @op (none))
  (comparison_operator (none) operators: ["==" "!="] @op (_) @left)
] @cmp
```

**Facts that must hold**

- `field_count(@cmp, 'operators') == 1`

**Question:** Can `${left}` only ever hold ordinary objects whose `==` is not redefined (no NumPy arrays, pandas objects or ORM columns)?

- **yes**: apply the correction
- **no**: keep the code, recorded as intentional

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,7 +1,7 @@
 def describe(value):
-    if value == None:
+    if value is None:
         return "missing"
-    if None != value:
+    if None is not value:
         return "present"
 
 
```

**Counterexample: already an identity test** (no finding)

```python
value = 1
print(value is None)
```

**Counterexample: a chained comparison** (no finding)

```python
x = None
print(x == None == True)
```


## redundant-list-around-sorted

**list() around sorted()** · `python-core/redundant-list-around-sorted`

category: waste · action: corrects (preserved)

`sorted()` always returns a new list, so wrapping it in `list()` copies that
list once more for nothing. The rule applies only when neither `list` nor
`sorted` is redefined in the file.

**Shape**

```scheme
(call
  function: (identifier) @outer
  arguments: (argument_list . (call function: (identifier) @inner_name) @inner .)
  (#eq? @outer "list")
  (#eq? @inner_name "sorted")) @call
```

**Facts that must hold**

- `!py.bound('list')`
- `!py.bound('sorted')`
- `!py.dynamic_names()`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,3 +1,3 @@
 names = ["bob", "ada", "cy"]
-print(list(sorted(names)))
-print(list(sorted(names, key=len, reverse=True))[0])
+print(sorted(names))
+print(sorted(names, key=len, reverse=True)[0])
```

**Counterexample: list() around something else** (no finding)

```python
print(list(reversed([1, 2])))
```

**Counterexample: sorted is redefined** (no finding)

```python
def sorted(items):
    return items

print(list(sorted((3, 1))))
```

**Counterexample: a local list() may do anything** (no finding)

```python
def list(xs):
    return tuple(xs)

print(list(sorted([3, 1])))
```


## redundant-pass

**Redundant pass** · `python-core/redundant-pass`

category: waste · action: corrects (preserved)

`pass` exists only to fill a block that would otherwise be empty. In a block
that already has a statement (a docstring counts) it does nothing, and it
usually marks a placeholder that was never cleaned up.

**Shape**

```scheme
(block (pass_statement) @pass)
```

**Facts that must hold**

- `count(children(parent(@pass))) > 1`
- `occupies_lines(@pass)`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,11 +1,9 @@
 class Config:
     """Settings."""
-    pass
 
 
 def run():
     print("running")
-    pass
 
 
 run()
```

**Counterexample: the only statement of a block** (no finding)

```python
def stub():
    pass
```


## return-bool-if-negated

**if/else that returns False or True** · `python-core/return-bool-if-negated`

category: simplification · action: corrects (preserved)

`if c: return False else: return True` returns `not c`. `not` always yields a
real bool and evaluates `c` once, so the two are identical. Where Python
defines a direct opposite (`not (a in b)` is `a not in b`, `not (a is b)` is
`a is not b`), the opposite operator is used.

**Shape**

```scheme
[
  (if_statement
    condition: (_) @cond
    consequence: (block . (return_statement (false)) .)
    alternative: (else_clause body: (block . (return_statement (true)) .))) @whole
  (block
    (if_statement
      condition: (_) @cond
      consequence: (block . (return_statement (false)) .)
      !alternative) @whole
    .
    (return_statement (true)) @whole)
]
```

**Facts that must hold**

- `occupies_lines(@whole)`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,14 +1,9 @@
 def is_allowed(user, banned):
-    if user in banned:
-        return False
-    else:
-        return True
+    return user not in banned
 
 
 def is_small(n):
-    if n > 100 or n < -100:
-        return False
-    return True
+    return not (n > 100 or n < -100)
 
 
 print(is_allowed("ada", {"bob"}), is_allowed("bob", {"bob"}))
```

**Counterexample: returns values other than booleans** (no finding)

```python
def f(n):
    if n:
        return 0
    return 1
```


## return-bool-if

**if/else that returns True or False** · `python-core/return-bool-if`

category: simplification · action: corrects (preserved)

`if c: return True else: return False` returns the truth value of `c`, which
is exactly what `bool(c)` computes: `if` decides by calling the same
`__bool__` that `bool()` calls, and evaluates `c` once, as `bool(c)` does.

`bool()` is dropped only when the condition always yields True or False by
itself (`not`, `in`, `not in`, `is`, `is not`). It is kept around other
comparisons because classes can redefine `==`, `<` and the rest to return
something that is not a bool, and the if/else would still have returned a
real bool.

**Shape**

```scheme
[
  (if_statement
    condition: (_) @cond
    consequence: (block . (return_statement (true)) .)
    alternative: (else_clause body: (block . (return_statement (false)) .))) @whole
  (block
    (if_statement
      condition: (_) @cond
      consequence: (block . (return_statement (true)) .)
      !alternative) @whole
    .
    (return_statement (false)) @whole)
]
```

**Facts that must hold**

- `occupies_lines(@whole)`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,14 +1,9 @@
 def is_admin(user, admins):
-    if user in admins:
-        return True
-    else:
-        return False
+    return user in admins
 
 
 def is_large(n):
-    if n > 100:
-        return True
-    return False
+    return bool(n > 100)
 
 
 class Odd:
```

**Counterexample: an elif chain** (no finding)

```python
def sign(n):
    if n > 0:
        return True
    elif n < 0:
        return False
    else:
        return None
```

**Counterexample: more work before returning** (no finding)

```python
def check(n):
    if n:
        print("yes")
        return True
    return False
```


## undefined-name

**Name that is never defined** · `python-core/undefined-name`

category: broken · action: explains

Nothing in the file binds this name: no import, assignment, parameter,
function, class, loop variable or global declaration, and it is not a
Python builtin. The file cannot create names at run time either. So every
execution that reaches this line fails with NameError.

No correction is applied because the fix depends on what was meant: a missing
import from your own code, a typo for another name, or a variable that should
have been assigned first. Reverse Codemosis imports standard-library names
on its own only when exactly one module can provide them (see
python-core/missing-module-import and python-core/missing-name-import).

**Shape**

```scheme
(identifier) @name
```

**Facts that must hold**

- `py.is_reference(@name)`
- `!py.bound(text(@name))`
- `!py.builtin(text(@name))`
- `!py.dynamic_names()`
- `!(in_table('stdlib_names', text(@name)) && !py.defined_in_project(text(@name), lookup('stdlib_names', text(@name))))`
- `!(in_table('stdlib_modules', text(@name)) && subset(py.attributes(text(@name)), table('stdlib_modules', text(@name))) && !py.defined_in_project(text(@name), text(@name)))`

**Example: 1**

```python
def total(prices):
    return sum(prices) * (1 + tax_rate)
```

**Example: a helper that was never imported**

```python
def report(rows):
    return format_table(rows)
```

**Counterexample: comprehension variables are bound** (no finding)

```python
def total(prices):
    return sum(price for price in prices)
```

**Counterexample: fixable imports are left to the import rules** (no finding)

```python
print(json.dumps({}))
```


## unreachable-code

**Unreachable code** · `python-core/unreachable-code`

category: waste · action: corrects (preserved) · CWE-561

A `return`, `raise`, `break` or `continue` always leaves the block it is in.
Statements after it in the same block never execute; they are usually the
remains of an earlier version, and they mislead readers into thinking they
matter.

Deleting statements that never run usually changes nothing, but Python has
two exceptions, and the rule checks both. A `yield` anywhere in a function
makes it a generator, even in dead code. And an assignment anywhere in a
function makes that name local to the whole function, even in dead code: if
the dead assignment is the only one, deleting it would make other reads of
the name find a global instead. When either is the case, nothing is
reported.

**Shape**

```scheme
(block [(return_statement) (raise_statement) (break_statement) (continue_statement)] @jump)
```

**Facts that must hold**

- `exists(@dead)`
- `occupies_lines(@dead)`
- `py.removal_preserves_scope(@dead)`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,8 +1,6 @@
 def total(prices):
     result = sum(prices)
     return result
-    print("computed", result)
-    result = round(result, 2)
 
 
 print(total([1.5, 2.25]))
```

**Example: after raise and continue**

```diff
--- a/example.py
+++ b/example.py
@@ -2,10 +2,8 @@
     for v in values:
         if v < 0:
             raise ValueError(v)
-            v = 0
         if v == 0:
             continue
-            print("zero")
         print(v)
 
 
```

**Counterexample: a dead yield still makes a generator** (no finding)

```python
def gen():
    return
    yield 1


print(list(gen()))
```

**Counterexample: a dead assignment still makes the name local** (no finding)

```python
x = "global"


def show():
    print(x)
    return
    x = "local"


try:
    show()
except UnboundLocalError:
    print("x is local to show()")
```

**Counterexample: code on the same line** (no finding)

```python
def f():
    return 1; print("never")
```


## unused-import

**Unused import** · `python-core/unused-import`

category: waste · action: corrects (preserved) · CWE-1164

Nothing in the file reads the imported name. The import still costs a module
load at start-up and tells readers the module matters when it does not.

Removing an import is safe only when importing has no effect besides
binding the name, and when nothing else can use the name. The rule
therefore removes only top-level imports of standard-library modules known
to have no import side effects, and only when the name appears nowhere else
in the file (not in code, strings, comments, `__all__` or doctests), the
file is not a package `__init__.py`, the file cannot look names up at run
time, and no other file in the project imports the name from this module.

**Shape**

```scheme
[
  (import_statement . name: (dotted_name . (identifier) @name) @module .)
  (import_statement . name: (aliased_import name: (dotted_name) @module alias: (identifier) @name) .)
  (import_from_statement module_name: (dotted_name) @module . name: (dotted_name . (identifier) @name .) .)
  (import_from_statement module_name: (dotted_name) @module . name: (aliased_import alias: (identifier) @name) .)
] @stmt
```

**Facts that must hold**

- `py.top_level(@stmt)`
- `in_table('side_effect_free_modules', text(@module))`
- `py.references(text(@name)) == 0`
- `py.bindings(text(@name)) == 1`
- `!py.mentioned(text(@name))`
- `!py.dynamic_names()`
- `file_name() != '__init__.py'`
- `!py.bound('__all__')`
- `!py.imported_elsewhere(text(@name))`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,6 +1,3 @@
-import os
 import json
-from collections import OrderedDict
-import re as regex
 
 print(json.dumps([1, 2]))
```

**Counterexample: a package re-exports its imports** (no finding)

```python
from os import path
```

**Counterexample: used only inside an f-string** (no finding)

```python
import os

print(f"{os.sep}")
```

**Counterexample: named in a string annotation** (no finding)

```python
import os


def open_dir(p: "os.PathLike"):
    return p
```

**Counterexample: an import with side effects** (no finding)

```python
import readline
```

**Counterexample: a guarded optional import** (no finding)

```python
try:
    import json
except ImportError:
    json = None
```


## vacuous-assert

**Assertion that can never fail** · `python-core/vacuous-assert`

category: contradiction · action: corrects (corrected) · CWE-571

`assert (condition, "message")` does not assert the condition. The
parentheses build a tuple of two items, and a non-empty tuple is always
true, so the assertion passes no matter what. CPython warns about it
(SyntaxWarning: assertion is always true).

The intent is unambiguous: assert the condition, with the message. The
correction removes the parentheses so the assertion checks what it says.
If the condition is in fact false somewhere, the assertion now fails there,
which is exactly what it was written to do.

**Shape**

```scheme
(assert_statement . (tuple . (_) @cond . (_) @msg .) @tuple .) @stmt
```

**Facts that must hold**

- `kind(@cond) not in ['named_expression', 'list_splat', 'yield']`
- `kind(@msg) not in ['named_expression', 'list_splat', 'yield']`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,5 +1,5 @@
 def withdraw(balance, amount):
-    assert (amount <= balance, "insufficient funds")
+    assert amount <= balance, "insufficient funds"
     return balance - amount
 
 
```

**Counterexample: an ordinary assertion** (no finding)

```python
x = 1
assert x > 0, "positive"
```

**Counterexample: comparing a tuple is a real check** (no finding)

```python
pair = (1, 2)
assert (1, 2) == pair
```

