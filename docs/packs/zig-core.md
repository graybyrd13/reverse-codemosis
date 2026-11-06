# zig-core 0.1.0

> Questions for Zig: debug printing left in library code.

**Languages:** zig · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`debug-print`](#debug-print) std.debug.print left in code | waste | asks, then corrects as chosen (preserved) | 1 ex. · 3 counter |

## debug-print

**std.debug.print left in code** · `zig-core/debug-print`

category: waste · action: asks, then corrects as chosen (preserved) · tags: debugging

`std.debug.print` writes to standard error without any error handling: it is meant for debugging, and easy
to leave in library code. Real output goes through a writer the caller chooses, and diagnostics through
`std.log`. The engine asks: keep records it as intentional; remove deletes the line.

**Shape**

```scheme
(expression_statement (call_expression function: (field_expression) @function)) @stmt
```

**Facts that must hold**

- `text(@function) == 'std.debug.print'`
- `file_purpose() == 'library'`
- `occupies_lines(@stmt)`

**Question:** Keep this std.debug.print?

- **keep**: it is wanted output; record it as intentional
- **remove**: it is leftover debugging; delete it

**Example: 1** (answer: remove)

```diff
--- a/src/cart.zig
+++ b/src/cart.zig
@@ -3,6 +3,5 @@
 pub fn total(items: []const u32) u32 {
     var sum: u32 = 0;
     for (items) |item| sum += item;
-    std.debug.print("sum={d}\\n", .{sum});
     return sum;
 }
```

**Counterexample: logging through std.log** (no finding)

```text
const std = @import("std");

pub fn total(items: []const u32) u32 {
    var sum: u32 = 0;
    for (items) |item| sum += item;
    std.log.info("sum={d}", .{sum});
    return sum;
}
```

**Counterexample: in an example, printing is the point** (no finding)

```text
const std = @import("std");

pub fn main() void {
    std.debug.print("hello\\n", .{});
    return;
}
```

**Counterexample: a statement that shares its line** (no finding)

```text
const std = @import("std");

pub fn total(a: u32, b: u32) u32 {
    std.debug.print("a\\n", .{}); return a + b;
}
```

