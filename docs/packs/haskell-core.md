# haskell-core 0.1.0

> Explanations for Haskell: undefined left in library code.

**Languages:** haskell · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`undefined`](#undefined) undefined in library code | hazard | explains | 1 ex. · 2 counter |

## undefined

**undefined in library code** · `haskell-core/undefined`

category: hazard · action: explains

`undefined` stands for a value that is not written yet: evaluating it throws an exception with no useful
message. In library code it is a crash waiting for the right input. Write the value, or use `error` with a
message that says what went wrong; for work in progress, a typed hole (`_`) makes the compiler say what is
missing. Nothing is changed automatically: only the author knows the value.

Tests are left alone: `undefined` is a common way to say "this argument is never used".

**Shape**

```scheme
((variable) @v (#eq? @v "undefined"))
```

**Facts that must hold**

- `file_purpose() == 'library'`
- `!hs.defines('undefined')`

**Example: 1**

```text
module Price where

discount :: Int -> Int
discount n = undefined
```

**Counterexample: a test** (no finding)

```text
module PriceSpec where

ignored :: Int
ignored = undefined
```

**Counterexample: undefined is defined here** (no finding)

```text
module Price where

undefined :: Int
undefined = 0

discount :: Int -> Int
discount n = n - undefined
```

