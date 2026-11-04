# python-legacy 0.1.0

> Python 2 constructs that have exactly one Python 3 equivalent.

Code copied from old answers and old repositories still carries Python 2
syntax that Python 3 rejects or misreads. Each rule here repairs one such
construct to its exact Python 3 form. Constructs whose Python 3 form is not
exact (a print statement with a trailing comma, which relied on the removed
soft-space mechanism) are left alone.

**Languages:** python · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`diamond-operator`](#diamond-operator) Python 2 <> operator | broken | corrects (repaired) | 1 ex. · 1 counter |
| [`except-comma`](#except-comma) Python 2 except clause with a comma | broken | corrects (repaired) | 1 ex. · 3 counter |
| [`exec-statement`](#exec-statement) Python 2 exec statement | broken | corrects (repaired) | 1 ex. · 1 counter |
| [`print-chevron`](#print-chevron) Python 2 print to a file | broken | corrects (repaired) | 1 ex. · 1 counter |
| [`print-statement`](#print-statement) Python 2 print statement | broken | corrects (repaired) | 1 ex. · 2 counter |
| [`renamed-builtin`](#renamed-builtin) Python 2 builtin renamed in Python 3 | broken | corrects (repaired) | 1 ex. · 1 counter |

## diamond-operator

**Python 2 <> operator** · `python-legacy/diamond-operator`

category: broken · action: corrects (repaired)

Python 2 accepted `<>` as a synonym for `!=`. Python 3 removed it, so the
file does not even compile under Python 3. Both operators call the same
`__ne__` method in Python 2, so replacing `<>` with `!=` is the only repair
and it changes nothing about what the comparison means.

**Shape**

```scheme
(comparison_operator operators: "<>" @op) @cmp
```

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1 +1 @@
-print(1 <> 2)
+print(1 != 2)
```

**Counterexample: 1** (no finding)

```python
print(1 != 2)
```


## except-comma

**Python 2 except clause with a comma** · `python-legacy/except-comma`

category: broken · action: corrects (repaired)

Python 2 accepted `except Error, name:` to bind the exception to `name`.
Python 3 up to 3.13 refuses to compile it. Since 3.14 (PEP 758) the same text
compiles but means something else: catch `Error` *or* an exception class
called `name`. Then, when an exception reaches the handler, evaluating the
nonexistent class `name` raises NameError instead.

The rule applies only when `name` cannot be an exception class: it is not a
builtin, nothing else in the file binds it, and the file cannot create names
at run time. Then the Python 2 meaning, `except Error as name:`, is the only
one under which the handler can work. `except ValueError, TypeError:` is
left alone: in Python 3.14 that is correct code.

**Shape**

```scheme
(except_clause value: (_) @type @head "," @comma value: (identifier) @name @head) @_clause
```

**Facts that must hold**

- `py.bindings(text(@name)) == 1`
- `!py.builtin(text(@name))`
- `!py.dynamic_names()`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,4 +1,4 @@
 try:
     int("x")
-except ValueError, err:
+except ValueError as err:
     print("bad number:", type(err).__name__)
```

**Counterexample: Python 3 syntax** (no finding)

```python
try:
    int("x")
except (ValueError, TypeError) as err:
    pass
```

**Counterexample: Python 3.14: two exception types** (no finding)

```python
try:
    int("x")
except ValueError, TypeError:
    pass
```

**Counterexample: Python 3.14: an exception class imported from elsewhere** (no finding)

```python
from errors import ParseError

try:
    int("x")
except ValueError, ParseError:
    pass
```


## exec-statement

**Python 2 exec statement** · `python-legacy/exec-statement`

category: broken · action: corrects (repaired)

Python 3 made exec a function: `exec code` is `exec(code)`, and the
statement form is a syntax error. With a single operand both forms run the
code in the current scope, so the call is the only repair. The forms with
`in globals, locals` are left alone, because Python 2 also accepted a tuple
operand there and the translation depends on which was meant.

**Shape**

```scheme
(exec_statement code: (_) @code) @stmt
```

**Facts that must hold**

- `count(children(@stmt)) == 1`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1 +1 @@
-exec "print(40 + 2)"
+exec("print(40 + 2)")
```

**Counterexample: 1** (no finding)

```python
exec("print(1)")
```


## print-chevron

**Python 2 print to a file** · `python-legacy/print-chevron`

category: broken · action: corrects (repaired)

`print >>stream, a, b` writes to `stream`; in Python 3 that is
`print(a, b, file=stream)`, and the chevron form is a syntax error. The
items, their separators and the final newline are the same in both, so the
call is the only repair. A trailing comma (which suppressed the newline in
Python 2) is left to a person, since it needs `end=" "`.

**Shape**

```scheme
(print_statement (chevron (_) @dest)) @stmt
```

**Facts that must hold**

- `exists(@args)`
- `!ends_with(text(@stmt), ',')`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,2 +1,2 @@
 import sys
-print >>sys.stdout, "to", "stdout"
+print("to", "stdout", file=sys.stdout)
```

**Counterexample: Python 3 print to a file** (no finding)

```python
import sys
print("x", file=sys.stderr)
```


## print-statement

**Python 2 print statement** · `python-legacy/print-statement`

category: broken · action: corrects (repaired)

Python 3 made print a function. `print a, b` prints its arguments separated
by spaces followed by a newline, which is exactly what `print(a, b)` does.
A statement ending in a comma is left alone: it suppressed the newline
through Python 2's soft-space mechanism, which has no exact Python 3 form.

**Shape**

```scheme
(print_statement) @stmt
```

**Facts that must hold**

- `exists(@args)`
- `!contains(@stmt, 'chevron')`
- `!ends_with(text(@stmt), ',')`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,2 +1,2 @@
 total = 3 * 4
-print "total:", total
+print("total:", total)
```

**Counterexample: already a function call** (no finding)

```python
print("hello")
```

**Counterexample: a trailing comma has no exact Python 3 form** (no finding)

```python
print "no newline",
```


## renamed-builtin

**Python 2 builtin renamed in Python 3** · `python-legacy/renamed-builtin`

category: broken · action: corrects (repaired)

Python 3 renamed `xrange` to `range` and `raw_input` to `input`, with the
same behaviour. Under Python 3 the old names raise NameError. The rule
applies only when the file defines neither the old nor the new name itself.

**Shape**

```scheme
(call
  function: (identifier) @old
  (#any-of? @old "xrange" "raw_input"))
```

**Facts that must hold**

- `!py.bound(text(@old))`
- `!py.bound(text(@new))`
- `!py.dynamic_names()`

**Example: 1**

```diff
--- a/example.py
+++ b/example.py
@@ -1,4 +1,4 @@
 total = 0
-for i in xrange(5):
+for i in range(5):
     total += i
 print(total)
```

**Counterexample: a compatibility shim defines the name** (no finding)

```python
try:
    xrange
except NameError:
    xrange = range

print(list(xrange(3)))
```

