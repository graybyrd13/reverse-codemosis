# rust-core 0.1.0

> Provable simplifications for everyday Rust, and the stubs AI tools leave behind.

Syntactic corrections whose equivalence follows from the Rust reference,
checked by compiling and running every example with rustc. The compiler is
the oracle for everything else: run Reverse Codemosis with `cargo build`
and `cargo test` configured and no step is kept unless both still pass.

**Languages:** rust · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |
| [`manual-assert`](#manual-assert) if !condition { panic!(..) } | simplification | corrects (preserved) | 1 ex. · 2 counter |
| [`needless-return`](#needless-return) return as the last statement of a function | simplification | corrects (preserved) | 1 ex. · 2 counter |
| [`redundant-field-init`](#redundant-field-init) Redundant field name in struct initializer | simplification | corrects (preserved) | 1 ex. · 1 counter |
| [`unfinished-stub`](#unfinished-stub) todo!() or unimplemented!() left in code | hazard | explains | 1 ex. · 1 counter |
| [`unit-return-type`](#unit-return-type) Explicit unit return type | simplification | corrects (preserved) | 1 ex. · 1 counter |

## fixed-wait

**Fixed wait** · `rust-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky, too
long and it is slow on every run. The safe form waits for the condition
itself (join the thread, await the future or channel, poll with a timeout).

Some waits are deliberate: rate limits, back-off, pacing. Only you know, so
the engine asks. keep records the wait as intentional; remove deletes it.
Waits inside loops and closures are not reported.

**Shape**

```scheme
(expression_statement
  (call_expression function: (scoped_identifier) @function) @call) @stmt

(expression_statement
  (await_expression (call_expression function: (scoped_identifier) @function)) @call) @stmt
```

**Facts that must hold**

- `matches(text(@function), '^((std::)?thread::sleep|(tokio::)?time::sleep|async_std::task::sleep)$')`
- `!inside(@stmt, 'loop_expression', 'while_expression', 'for_expression', 'closure_expression')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/example.rs
+++ b/example.rs
@@ -2,6 +2,5 @@
 
 fn main() {
     println!("started");
-    std::thread::sleep(Duration::from_millis(5));
     println!("finished");
 }
```

**Counterexample: a polling interval inside a loop** (no finding)

```rust
use std::time::Duration;

fn wait(ready: impl Fn() -> bool) {
    while !ready() {
        std::thread::sleep(Duration::from_millis(10));
    }
}
```


## manual-assert

**if !condition { panic!(..) }** · `rust-core/manual-assert`

category: simplification · action: corrects (preserved)

`assert!(cond, args..)` is defined as `if !cond { panic!(args..) }`, with
the same message formatting. Writing the assertion says what the code
checks rather than how it fails.

**Shape**

```scheme
(expression_statement
  (if_expression
    condition: (unary_expression "!" (_) @cond)
    consequence: (block .
      [
        (expression_statement (macro_invocation macro: (identifier) @mac (token_tree) @args))
        (macro_invocation macro: (identifier) @mac (token_tree) @args)
      ] .)
    !alternative)) @stmt
```

**Facts that must hold**

- `text(@mac) == 'panic'`
- `text(@args) != '()'`
- `starts_with(text(@args), '(')`

**Example: 1**

```diff
--- a/example.rs
+++ b/example.rs
@@ -1,7 +1,5 @@
 fn withdraw(balance: u32, amount: u32) -> u32 {
-    if !(amount <= balance) {
-        panic!("insufficient funds: {} > {}", amount, balance);
-    }
+    assert!(amount <= balance, "insufficient funds: {} > {}", amount, balance);
     balance - amount
 }
 
```

**Counterexample: an else branch** (no finding)

```rust
fn check(ok: bool) {
    if !ok {
        panic!("bad");
    } else {
        println!("good");
    }
}
```

**Counterexample: more than a panic** (no finding)

```rust
fn check(ok: bool) {
    if !ok {
        eprintln!("about to fail");
        panic!("bad");
    }
}
```


## needless-return

**return as the last statement of a function** · `rust-core/needless-return`

category: simplification · action: corrects (preserved)

A function body's final expression is its return value. `return value;` as
the very last statement does exactly what `value` as the tail expression
does, and is the form `cargo clippy` also recommends.

One case is excluded: an expression that begins with a block-like construct
(`if`, `match`, `loop`, a block...) and continues after it, such as
`return match x { .. } + 1;`, because in tail position Rust would parse the
leading construct as a statement of its own.

**Shape**

```scheme
(function_item
  body: (block
    (expression_statement (return_expression (_) @value)) @ret .)) @_fn
```

**Facts that must hold**

- `kind(@value) in ['if_expression', 'match_expression', 'block', 'loop_expression', 'unsafe_block'] || !matches(text(@value), '^((if|match|loop|while|for|unsafe|async)([^A-Za-z0-9_]|$)|[{])')`

**Example: 1**

```diff
--- a/example.rs
+++ b/example.rs
@@ -1,6 +1,6 @@
 fn double(x: i32) -> i32 {
     let y = x * 2;
-    return y;
+    y
 }
 
 fn main() {
```

**Counterexample: an early return** (no finding)

```rust
fn first_even(v: &[i32]) -> Option<i32> {
    for x in v {
        if x % 2 == 0 {
            return Some(*x);
        }
    }
    None
}
```

**Counterexample: a return followed by a comment** (no finding)

```rust
fn one() -> i32 {
    return 1;
    // done
}
```


## redundant-field-init

**Redundant field name in struct initializer** · `rust-core/redundant-field-init`

category: simplification · action: corrects (preserved)

In a struct expression, `Point { x: x }` and `Point { x }` are the same
expression by definition (field init shorthand, Rust Reference §8.2.8).

**Shape**

```scheme
(field_initializer
  field: (field_identifier) @field
  value: (identifier) @value
  (#eq? @field @value)) @init
```

**Example: 1**

```diff
--- a/example.rs
+++ b/example.rs
@@ -6,6 +6,6 @@
 fn main() {
     let x = 1;
     let y = 2;
-    let p = Point { x: x, y: y };
+    let p = Point { x, y };
     println!("{} {}", p.x, p.y);
 }
```

**Counterexample: fields from other names** (no finding)

```rust
struct Point { x: i32, y: i32 }

fn swap(p: Point) -> Point {
    let (x, y) = (p.x, p.y);
    Point { x: y, y: x }
}
```


## unfinished-stub

**todo!() or unimplemented!() left in code** · `rust-core/unfinished-stub`

category: hazard · action: explains

`todo!()` and `unimplemented!()` compile anywhere and panic at run time.
They are placeholders, and AI tools leave them behind in code that otherwise
looks complete. Nothing can be corrected automatically: the missing code is
a design decision. The finding marks exactly where it is missing.

**Shape**

```scheme
(macro_invocation
  macro: (identifier) @mac
  (#any-of? @mac "todo" "unimplemented")) @call
```

**Example: 1**

```rust
fn area(radius: f64) -> f64 {
    todo!()
}
```

**Counterexample: an implemented function** (no finding)

```rust
fn area(radius: f64) -> f64 {
    std::f64::consts::PI * radius * radius
}
```


## unit-return-type

**Explicit unit return type** · `rust-core/unit-return-type`

category: simplification · action: corrects (preserved)

A function with no return type returns `()`. Writing `-> ()` adds nothing
and makes the reader check whether something is returned.

**Shape**

```scheme
(function_item
  parameters: (parameters) @params @sig
  "->" @sig
  return_type: (unit_type) @sig) @_fn
```

**Example: 1**

```diff
--- a/example.rs
+++ b/example.rs
@@ -1,4 +1,4 @@
-fn greet(name: &str) -> () {
+fn greet(name: &str) {
     println!("hello, {}", name);
 }
 
```

**Counterexample: a real return type** (no finding)

```rust
fn one() -> i32 {
    1
}
```

