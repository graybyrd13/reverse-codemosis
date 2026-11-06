# dart-core 0.1.0

> Questions for Dart: print left in library code.

**Languages:** dart · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`print-in-library`](#print-in-library) print in library code | waste | asks, then corrects as chosen (preserved) | 1 ex. · 3 counter |

## print-in-library

**print in library code** · `dart-core/print-in-library`

category: waste · action: asks, then corrects as chosen (preserved) · tags: debugging

`print` in library code is usually debugging left behind (the `avoid_print` lint). Libraries report through
a logger the application configures, or `debugPrint` in Flutter. It may be real output, so the engine asks:
keep records it as intentional; remove deletes the line. Scripts, examples and tests are left alone.

**Shape**

```scheme
(expression_statement
  (call_expression function: (identifier) @function)
  (#eq? @function "print")) @stmt
```

**Facts that must hold**

- `file_purpose() == 'library'`
- `occupies_lines(@stmt)`
- `!dart.defines('print')`

**Question:** Keep this print?

- **keep**: it is real output; record it as intentional
- **remove**: it is leftover debugging; delete it

**Example: 1** (answer: remove)

```diff
--- a/lib/cart.dart
+++ b/lib/cart.dart
@@ -1,4 +1,3 @@
 int total(List<int> items) {
-  print(items);
   return items.fold(0, (a, b) => a + b);
 }
```

**Counterexample: in an example, printing is the point** (no finding)

```text
void main() {
  print('hello');
  return;
}
```

**Counterexample: a statement that shares its line** (no finding)

```text
int total(List<int> items) {
  print(items); return items.length;
}
```

**Counterexample: print is defined here** (no finding)

```text
void print(Object o) {}

void report(int n) {
  print(n);
  return;
}
```

