# java-core 0.1.0

> Provable corrections for Java, and questions where intent decides.

**Languages:** java · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`bool-literal-comparison`](#bool-literal-comparison) Comparison with true or false | simplification | corrects (preserved) | 1 ex. · 1 counter |
| [`empty-catch`](#empty-catch) Empty catch block | hazard | explains | 1 ex. · 1 counter |
| [`fixed-wait`](#fixed-wait) Fixed wait | hazard | asks, then corrects as chosen (preserved) | 1 ex. · 1 counter |
| [`string-reference-equality`](#string-reference-equality) String compared with == | contradiction | corrects (corrected) | 1 ex. · 1 counter |

## bool-literal-comparison

**Comparison with true or false** · `java-core/bool-literal-comparison`

category: simplification · action: corrects (preserved)

Java only lets `boolean` and `Boolean` values be compared with `true` and
`false`, so `x == true` is exactly `x` and `x == false` is exactly `!x` (a
null Boolean throws in both forms, the same way).

**Shape**

```scheme
(binary_expression
  left: (_) @value
  operator: ["==" "!="] @op
  right: [(true) (false)] @literal) @cmp

(binary_expression
  left: [(true) (false)] @literal
  operator: ["==" "!="] @op
  right: (_) @value) @cmp
```

**Facts that must hold**

- `kind(@value) not in ['true', 'false']`

**Example: 1**

```diff
--- a/Example.java
+++ b/Example.java
@@ -1,7 +1,7 @@
 public class Snippet {
     public static void main(String[] args) {
         boolean ready = args.length == 0;
-        if (ready == true) System.out.println("ready");
-        if (ready == false) System.out.println("not ready");
+        if (ready) System.out.println("ready");
+        if (!ready) System.out.println("not ready");
     }
 }
```

**Counterexample: 1** (no finding)

```java
public class Snippet {
    static boolean same(int a, int b) { return a == b; }
}
```


## empty-catch

**Empty catch block** · `java-core/empty-catch`

category: hazard · action: explains · CWE-390

An empty catch hides every exception of that type, including the unexpected
ones. When ignoring is right, a comment saying why makes it a decision (and
the rule stays quiet); otherwise handle, log or rethrow. Which one is a
design question, so nothing is changed.

**Shape**

```scheme
(catch_clause body: (block) @body) @clause
```

**Facts that must hold**

- `count(children(@body)) == 0`
- `!has_comment(@body)`

**Example: 1**

```java
class Loader {
    void load() {
        try {
            read();
        } catch (Exception e) {
        }
    }
    void read() throws Exception {}
}
```

**Counterexample: 1** (no finding)

```java
class Loader {
    void load() {
        try {
            read();
        } catch (Exception e) {
            // optional file; absence is fine
        }
    }
    void read() throws Exception {}
}
```


## fixed-wait

**Fixed wait** · `java-core/fixed-wait`

category: hazard · action: asks, then corrects as chosen (preserved) · tags: timing, flaky-tests

A fixed sleep is a guess about timing: too short and the code is flaky (what it waited for is not ready yet), too long and every run is slow. The safe form waits for the condition itself: join the thread, wait on the channel or future, poll with a timeout.

Some waits are deliberate (rate limits, back-off, pacing), so the engine asks. keep records the wait as intentional; remove deletes it. Waits inside loops are polling intervals and are not reported.

A sleep inside a try block is not reported: removing it could leave a catch
of InterruptedException that javac rejects as unreachable.

**Shape**

```scheme
(expression_statement
  (method_invocation object: (identifier) @class name: (identifier) @method) @call
  (#eq? @class "Thread")
  (#eq? @method "sleep")) @stmt

(expression_statement
  (method_invocation object: (field_access object: (identifier) @class) name: (identifier) @method) @call
  (#eq? @class "TimeUnit")
  (#eq? @method "sleep")) @stmt
```

**Facts that must hold**

- `!inside(@stmt, 'for_statement', 'enhanced_for_statement', 'while_statement', 'do_statement', 'lambda_expression')`
- `!inside(@stmt, 'try_statement', 'try_with_resources_statement')`
- `occupies_lines(@stmt)`

**Question:** Keep this fixed wait?

- **keep**: the wait is deliberate (rate limit, back-off, pacing); record it as intentional
- **remove**: delete it; if something must be ready first, wait for that condition instead

**Example: 1** (answer: remove)

```diff
--- a/Example.java
+++ b/Example.java
@@ -1,7 +1,6 @@
 public class Snippet {
     public static void main(String[] args) throws Exception {
         System.out.println("start");
-        Thread.sleep(10);
         System.out.println("done");
     }
 }
```

**Counterexample: inside a try block** (no finding)

```java
public class Snippet {
    static void pause() {
        try {
            Thread.sleep(10);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
    }
}
```


## string-reference-equality

**String compared with ==** · `java-core/string-reference-equality`

category: contradiction · action: corrects (corrected) · CWE-597

In Java `==` on objects asks whether both sides are the same object. String
literals are interned, so `s == "admin"` can pass in a quick test and then
fail for a string read from input, a file or a network call. The comparison
can only have meant equal text: `"admin".equals(s)`, which is also false
rather than throwing when `s` is null, exactly as `==` was.

**Shape**

```scheme
(binary_expression
  left: (_) @value
  operator: ["==" "!="] @op
  right: (string_literal) @literal) @cmp

(binary_expression
  left: (string_literal) @literal
  operator: ["==" "!="] @op
  right: (_) @value) @cmp
```

**Facts that must hold**

- `kind(@value) not in ['string_literal', 'null_literal']`

**Example: 1**

```diff
--- a/Example.java
+++ b/Example.java
@@ -1,7 +1,7 @@
 public class Snippet {
     public static void main(String[] args) {
         String role = new String("admin");
-        System.out.println(role == "admin");
-        System.out.println("admin" != role);
+        System.out.println("admin".equals(role));
+        System.out.println(!"admin".equals(role));
     }
 }
```

**Counterexample: 1** (no finding)

```java
public class Snippet {
    static boolean isAdmin(String role) { return "admin".equals(role); }
}
```

