# elixir-core 0.1.0

> Questions for Elixir: debugging output left in code.

**Languages:** elixir · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`debug-inspect`](#debug-inspect) IO.inspect left in code | waste | asks, then corrects as chosen (preserved) | 1 ex. · 4 counter |

## debug-inspect

**IO.inspect left in code** · `elixir-core/debug-inspect`

category: waste · action: asks, then corrects as chosen (preserved) · tags: debugging

`IO.inspect` prints its argument and returns it, which makes it the usual way to look at a value while
debugging, and easy to leave behind. It may also be real output, so the engine asks. keep records it as
intentional; remove deletes the line.

Only standalone calls are reported: an `IO.inspect` in a pipeline passes its value on, and the last
expression of a block is its result, so deleting either would change the program.

**Shape**

```scheme
(call
  target: (dot left: (alias) @module right: (identifier) @function)
  (#eq? @module "IO")
  (#eq? @function "inspect")) @call
```

**Facts that must hold**

- `file_purpose() in ['library', 'test']`
- `kind(parent(@call)) in ['do_block', 'stab_clause', 'body', 'source']`
- `exists(siblings_after(@call))`
- `occupies_lines(@call)`

**Question:** Keep this IO.inspect?

- **keep**: it is real output; record it as intentional
- **remove**: it is leftover debugging; delete it

**Example: 1** (answer: remove)

```diff
--- a/lib/cart.ex
+++ b/lib/cart.ex
@@ -1,6 +1,5 @@
 defmodule Cart do
   def total(items) do
-    IO.inspect(items)
     Enum.sum(items)
   end
 end
```

**Counterexample: in an example, printing is the point** (no finding)

```text
defmodule Cart do
  def total(items) do
    IO.inspect(items)
    Enum.sum(items)
  end
end
```

**Counterexample: the start of a pipeline: deleting the line would break it** (no finding)

```text
defmodule Cart do
  def total(items) do
    IO.inspect(items)
    |> Enum.sum()
  end
end
```

**Counterexample: the last expression is the function's result** (no finding)

```text
defmodule Cart do
  def show(items) do
    IO.inspect(items)
  end
end
```

**Counterexample: a call that shares its line** (no finding)

```text
defmodule Cart do
  def total(items) do
    IO.inspect(items); Enum.sum(items)
  end
end
```

