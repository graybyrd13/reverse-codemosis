# Reverse Codemosis

**Reverse Codemosis corrects code the way a proof corrects an argument.**

It is an open source Rust tool with no language model inside, and it never
guesses. Point it at a project and it works out the best form that code can
take, finds every place the code falls short of that form, fixes what it can
prove, explains what it cannot fix, and asks you for the missing fact when a
decision is genuinely yours.

```console
$ codemosis fix
```

```text
Baseline
  build     failed (1 error)
  tests     0 passed · 4 failed
  distance  broken 4 · contradiction 4 · hazard 2 · waste 7 · simplification 1

Corrections
  ✓  1  python-core/missing-module-import 2 corrections in inventory/report.py  build and tests unchanged
  ✓  2  python-core/missing-name-import inventory/store.py:27  tests 0 → 10 passing
  ✓  3  python-legacy/except-comma inventory/pricing.py:16  tests 10 → 18 passing
  ✓  4  python-core/is-literal inventory/models.py:31  tests 18 → 20 passing
  ✓  5  python-core/vacuous-assert inventory/store.py:24  tests 20 → 21 passing
  ✓  6  python-core/fstring-without-placeholders inventory/report.py:29  build and tests unchanged
  ✓  7  python-core/redundant-list-around-sorted inventory/store.py:44  build and tests unchanged
  ✓  8  python-core/redundant-pass inventory/models.py:8  build and tests unchanged
  ✓  9  python-core/unreachable-code inventory/store.py:40  build and tests unchanged
  ✓ 10  python-core/unused-import 3 corrections in inventory/pricing.py, inventory/store.py  build and tests unchanged
  ✓ 11  python-core/return-bool-if inventory/store.py:29  build and tests unchanged

Result
  build     ok
  tests     21 passed · 0 failed
  distance  broken 0 · contradiction 2 · hazard 2 · waste 0 · simplification 0
  applied   14 corrections in 4 files; review them with `git diff`

Needs your decision (3)
...
```

That is [the proof](examples/broken-inventory): one broken repository, one
command, a green test suite at the end, and [a diff](examples/broken-inventory.diff)
a careful human can read and approve line by line. The test suite checks that
it stays that way.

## The core idea

Every piece of code sits at some distance from the best version of itself.
That distance is not a matter of taste. It is made of things that can be
defined and measured:

| Category | What it means |
|---|---|
| **broken** | errors that break the build or fail every time the line runs |
| **contradiction** | logic that contradicts itself |
| **hazard** | unsafe patterns with known safe forms |
| **waste** | dead paths and wasted work |
| **simplification** | structures that a simpler structure provably replaces |

Reverse Codemosis defines the best form as a body of rules, measures real code
against it, and closes the distance step by step, most serious category
first, checking the build and the tests after every step so that no
correction ever makes the code worse.

## Every language GitHub knows

Reverse Codemosis identifies every file the way GitHub does, from
[Linguist](https://github.com/github-linguist/linguist)'s catalogue of
**845 languages** (by filename, shebang and extension, with content
heuristics where an extension is shared), and understands each at one of
three depths:

| Tier | Languages | What it understands |
|---|---|---|
| **deep** | 17: Python, Rust, JavaScript, TypeScript, TSX, Go, Java, C, C++, C#, Ruby, PHP, Shell, Kotlin, Scala, Lua, Swift | the full syntax tree, scopes and declarations, the language's own facts; examples are compiled and run |
| **lexical** | 343, from PowerShell and R to Erlang, T-SQL, Makefiles and YAML | what is code, what is a comment and what is a string, derived from the editor modes Linguist records |
| **recognized** | the other 485 | the language's identity; universal rules such as unresolved merge conflicts |

`codemosis languages` lists all 845 with their tier. Detection is itself
held to the rule of not guessing: when an extension is shared by several
languages (`.h`, `.m`, `.pl`, `.R`...) and nothing in the file decides
between them, the file is marked uncertain and no language-specific rule
runs on it. Generated files (lockfiles, `*.pb.go`, "DO NOT EDIT" headers,
minified code) and vendored code are never edited.

```console
$ codemosis map
```

```text
Languages (by size; percentages count programming and markup, as GitHub does)
  ● JavaScript             100.0%    4 files  deep: syntax tree and facts (javascript) · 11 rules
  ● Markdown                    -    1 file   recognized: universal rules only · 2 rules
  ● JSON                        -    1 file   recognized: universal rules only · 2 rules

Build and test
  ecosystems  JavaScript/TypeScript (npm)
  oracle      detected
    test      npm test

Definitions (deep languages)
  JavaScript     1 classes · 2 functions · 5 methods

Distance from best form
  broken 0 · contradiction 2 · hazard 2 · waste 3 · simplification 1
  4 provably fixable · 3 need your decision · 12 rules from 3 packs apply
```

The build and test commands are found from the project's own files
(`Cargo.toml`, `go.mod`, `package.json` scripts, Maven, Gradle, .NET, Mix,
SwiftPM, Bundler, Composer, Python, Make), and individual test results are
read from unittest, pytest, cargo, Go, TAP, Jest/Vitest/Mocha and dotnet.

## Questions

Where a correction depends on a fact only you have, Reverse Codemosis asks.
Questions are yes/no, or offer named choices, each saying what it means:

```console
$ codemosis fix --interactive
```

```text
3 questions need a decision only you can make.

[1/3]
hazard         javascript-core/fixed-wait  Fixed wait
  --> src/checkout.js:14:3
     |
  14 |   await new Promise((resolve) => setTimeout(resolve, 300));
     |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  = `await new Promise((resolve) => setTimeout(resolve, 300))` pauses for a fixed time instead of waiting for what the code actually needs.
  Keep this fixed wait?
    keep     the wait is deliberate (rate limit, back-off, pacing); record it as intentional
    remove   delete it; if something must be ready first, wait for that condition instead
  answer [keep/remove/skip/quit] remove
  recorded; `codemosis fix` will apply it
```

Fixed waits (sleeps, `setTimeout` promises, `Thread.sleep`, `Start-Sleep`,
`WAITFOR DELAY`, `sleep 10` in a Makefile...) are asked about in every deep
language and, through one table anyone can extend, in 32 more; waits that
pace a polling loop are left alone. Other questions cover mutable default arguments, bare `except`,
strings that look like they meant to be f-strings, leftover `console.log`,
`var_dump` and breakpoints, unquoted variables in `rm`, assignments in
conditions, and which side of an unresolved merge conflict to keep.

Every answer is recorded in `.codemosis/answers.toml` with the question, the
rule and the location, reviewed and committed like code. Answers that keep
the code record it as intentional, and it is not asked again.

## How it works

It has two parts.

**The engine** (Rust) reads a whole project, identifies and parses every
file and holds it as one connected structure, so facts can look across
files. It applies a transformation only when every condition is met, then
verifies the result against the project's own build and tests.

**Fix packs** are libraries of small formal rules that anyone can write and
share, each pack covering one language, framework or family of failures. A
rule says: *when this shape appears and these facts hold, this correction is
safe, and here is the proof*. Shapes are tree-sitter queries for deep
languages, or regular expressions that must start in code (not in a comment
or a string) for any language.

```toml
id = "is-literal"
title = "Identity comparison with a literal"
category = "contradiction"
semantics = "corrected"
summary = "`${cmp}` asks whether two objects are the same object in memory, not whether they are equal."
explain = "..."

# The shape: a tree-sitter query.
shape = '''
[
  (comparison_operator (_) @left operators: ["is" "is not"] @op
                       [(integer) (float) (string) (concatenated_string)] @literal)
  (comparison_operator [(integer) (float) (string) (concatenated_string)] @literal
                       operators: ["is" "is not"] @op (_) @right)
] @cmp
'''
# The facts that must hold.
where = ["field_count(@cmp, 'operators') == 1"]

[let]
replacement = "choose(text(@op) == 'is', '==', '!=')"

# The correction.
[fix]
edits = [{ replace = "@op", with = "${replacement}" }]

# The proof: run before and after; `after` must print exactly this.
[[example]]
before = '''
status = "".join(["act", "ive"])
print(status is "active")
'''
after = '''
status = "".join(["act", "ive"])
print(status == "active")
'''
output = "True\n"

# Where the rule stops.
[[counterexample]]
code = '''
value = None
print(value is None)
'''
```

Every rule proves itself before it ships. `codemosis pack test` applies each
rule to its examples until it stops matching, compares the result with
`after` exactly, checks that the result parses and that nothing is left to
correct, and then **runs** `before` and `after` with the real interpreter or
compiler (python3, node, go, javac/java, cc, c++, ruby, php, bash, rustc) to
check the rule's claim:

- `preserved`: both produce the same output and exit status;
- `corrected`: `after` produces the declared output and `before` does not;
- `repaired`: `before` fails to compile or run, and `after` runs.

Counterexamples must not match at all.

### The acceptance contract

On a real project, the engine keeps a step only if:

1. every finding it targets is gone;
2. it introduces no finding of a more serious category;
3. every file it touches still parses, with no new syntax errors;
4. the build does not regress;
5. no test that passed before stops passing.

A step that fails is rolled back. If it held several corrections, the engine
bisects to find the one at fault, rejects that one with its evidence, and
keeps the rest. Rule proofs cover what the rule's semantic model covers; the
oracle covers everything else, such as a test that reads source text.

### Refusing to guess

Where a correction cannot be proven, the tool does not improvise. It reports
the problem in plain language, shows the evidence, and asks for the detail
that would settle it. **That refusal to guess is the product.**

Facts are conservative by construction: when one cannot be sure, it blocks
the correction. A missed fix is acceptable. A wrong fix is not. Some
examples of what that means in practice:

- `python-core/unreachable-code` will not delete `x = 1` after a `return`
  if that dead line is the only thing making `x` local to the function.
- `javascript-core/unreachable-code` will not delete dead code that declares
  anything: function declarations and `var` hoist, and `let`/`const` cast a
  temporal dead zone back over the whole block.
- `python-legacy/except-comma` leaves `except A, B:` alone, because since
  Python 3.14 that is valid code catching two exception types.
- `go-core/fixed-wait` only asks about a `time.Sleep` when removing it would
  not leave the `time` import unused, which Go refuses to compile.

## What it refuses to do

It does not design. It has no brain and pretends to none. It will not invent
features, choose architectures, or write your program from a wish. It takes
code that exists, including the flood of almost-right code now pouring out of
AI tools, and returns code that is correct, clean, and as close to optimal as
the rules can prove. Design is a later chapter that begins only once
correction is trusted completely.

## Getting started

```console
$ cargo install --path crates/codemosis-cli   # installs `codemosis`
$ cd your-project                             # a clean git working tree
$ codemosis map                               # what it understands about the project
$ codemosis check                             # every finding, nothing changed
$ codemosis fix --interactive                 # answer the questions, then apply what can be proven
$ git diff                                    # read it
```

| Command | What it does |
|---|---|
| `codemosis check [PATH]` | Measure the project and report every finding. Changes nothing. |
| `codemosis fix [PATH]` | Apply every provable correction, verifying each step. `--interactive` asks the questions first; `--dry-run` plans without writing. |
| `codemosis ask [PATH]` | Ask the open questions one at a time and record the answers. |
| `codemosis answer ID ANSWER` | Record one answer: `yes`, `no`, `keep`, `remove`, `ours`... |
| `codemosis map [PATH]` | What the engine understands: languages and tiers, build and tests, definitions, protected files. |
| `codemosis languages` | Every language GitHub knows, with how deeply each is understood. |
| `codemosis explain RULE` | What a rule finds, why, the facts it needs and its proof. |
| `codemosis rules` | List the rules of every pack. |
| `codemosis pack test [DIR]` | Run a pack's self-tests (the built-in packs by default). |
| `codemosis pack new DIR --id ID` | Start a pack from a working template. |
| `codemosis tree FILE` | Print a file's syntax tree, to help write shapes. |
| `codemosis facts` | The fact vocabulary, template filters and edit operations. |
| `codemosis init` | Write a `codemosis.toml` with the detected build and test commands. |

`fix` works in place, so by default it refuses to run on a dirty working
tree: `git diff` then shows exactly what Reverse Codemosis did, and
`git checkout .` undoes it.

## Built-in packs

16 packs, 62 rules, 365 self-test checks.

| Pack | Covers |
|---|---|
| [`python-core`](packs/python-core) | missing imports, undefined names, `is` with literals, vacuous asserts, duplicate keys, missing f-prefixes, mutable defaults, bare except, `== None`, unreachable code, unused imports, redundant code, boolean returns, fixed waits |
| [`python-legacy`](packs/python-legacy) | Python 2 constructs with exact Python 3 equivalents |
| [`javascript-core`](packs/javascript-core) | JavaScript, TypeScript and TSX: `=== NaN`, unreachable code (hoisting-aware), `debugger`, boolean returns, assignments in conditions, duplicate keys, empty catch, `console.log`, fixed waits |
| [`go-core`](packs/go-core) | comparisons with `true`/`false`, needless `return`, fixed waits |
| [`java-core`](packs/java-core) | strings compared with `==`, comparisons with `true`/`false`, empty catch, fixed waits |
| [`c-family`](packs/c-family) | C and C++: assignments in conditions, fixed waits |
| [`csharp-core`](packs/csharp-core), [`kotlin-core`](packs/kotlin-core), [`scala-core`](packs/scala-core), [`swift-core`](packs/swift-core) | fixed waits; empty catch in C# |
| [`ruby-core`](packs/ruby-core) | fixed waits, breakpoints left behind |
| [`php-core`](packs/php-core) | fixed waits, `var_dump`/`dd` left behind |
| [`shell-core`](packs/shell-core) | fixed waits, unquoted variables in `rm` |
| [`rust-core`](packs/rust-core) | needless `return`, field-init shorthand, `-> ()`, manual `assert!`, `todo!()`, fixed waits |
| [`universal`](packs/universal) | every text file: unresolved merge conflicts, committed private keys |
| [`waits`](packs/waits) | fixed waits in 32 more languages (PowerShell, R, T-SQL, Makefiles, YAML...), from a table anyone can extend |

## The demonstrations

- [`examples/broken-inventory`](examples/broken-inventory) (Python): does
  not build, every test fails; `codemosis fix` makes it green with 14
  verified corrections, 3 questions and 1 explanation.
  [The diff.](examples/broken-inventory.diff)
- [`examples/flaky-checkout`](examples/flaky-checkout) (JavaScript): one
  test fails and the suite is slowed by fixed waits; `codemosis fix
  --interactive` asks three questions, then makes it green and fast.
  [The diff.](examples/flaky-checkout.diff)

## Project status

This is the foundation: the engine, the pack format and its proof system,
the verification loop, language identification for everything GitHub knows,
17 deep languages, 16 packs and two demonstrations. See
[docs/ROADMAP.md](docs/ROADMAP.md) for what comes next.

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): how the engine is put together
- [docs/PACK_FORMAT.md](docs/PACK_FORMAT.md): how to write a fix pack
- [docs/SEMANTICS.md](docs/SEMANTICS.md): what "proven" means here, precisely
- [docs/FACTS.md](docs/FACTS.md): the fact vocabulary reference
- [CONTRIBUTING.md](CONTRIBUTING.md): how to contribute rules and code

## Why it matters

IDEs tell you what is wrong and leave the work to you. AI assistants try
their best and hand back fresh slop with total confidence. Reverse Codemosis
is the third thing: tireless, exact, and honest about its limits. Every AI
tool that ships sloppy code makes it more necessary, every fix pack a
stranger contributes makes it stronger, and every developer who has ever
inherited a broken project is its audience.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. Contributions are accepted under
the same terms. The language catalogue in [`data/linguist`](data/linguist)
is GitHub Linguist's, used under its MIT license.
