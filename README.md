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
hazard         python-core/mutable-default  Mutable default argument
  --> inventory/store.py:40:54
     |
  40 |     def restock(self, sku: str, amount: int, history=[]):
     |                                                      ^^
  = `history=[]` is evaluated once, when `restock` is defined; every call that omits `history` shares that one object.
  = question Q-95adbf57
    Should every call to `restock` that leaves out `history` get a fresh `[]`, instead of all such calls sharing one? (An explicit `history=None` will then also mean the default.)
    answer with: codemosis answer Q-95adbf57 yes   (or no)
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

## How it works

It has two parts.

**The engine** (Rust) reads a whole project, parses every file with
tree-sitter and holds it as one connected structure, so facts can look
across files. It applies a transformation only when every condition is met,
then verifies the result against the project's own build and tests.

**Fix packs** are libraries of small formal rules that anyone can write and
share, each pack covering one language, framework or family of failures. A
rule says: *when this shape appears and these facts hold, this correction is
safe, and here is the proof*.

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
`after` exactly, checks that the result parses and that the rule no longer
matches, and then **runs** `before` and `after` with the real interpreter or
compiler to check the rule's claim:

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
that would settle it. Answers are recorded in `.codemosis/answers.toml`,
reviewed and committed like code. **That refusal to guess is the product.**

Facts are conservative by construction: when one cannot be sure, it blocks
the correction. A missed fix is acceptable. A wrong fix is not. For example,
`python-core/unreachable-code` will not delete `x = 1` after a `return` if
that dead line is the only thing making `x` local to the function, because
deleting it would change which `x` the rest of the function reads.

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
$ codemosis init                              # writes codemosis.toml with your build and test commands
$ codemosis check                             # every finding, nothing changed
$ codemosis fix                               # apply what can be proven, verified step by step
$ git diff                                    # read it
```

| Command | What it does |
|---|---|
| `codemosis check [PATH]` | Measure the project and report every finding. Changes nothing. |
| `codemosis fix [PATH]` | Apply every provable correction, verifying each step. `--dry-run` plans without writing. |
| `codemosis answer ID yes\|no` | Record a fact only you can supply. |
| `codemosis explain RULE` | What a rule finds, why, the facts it needs and its proof. |
| `codemosis rules` | List the rules of every pack. |
| `codemosis pack test [DIR]` | Run a pack's self-tests (the built-in packs by default). |
| `codemosis pack new DIR --id ID` | Start a pack from a working template. |
| `codemosis tree FILE` | Print a file's syntax tree, to help write shapes. |
| `codemosis facts` | The fact vocabulary, template filters and edit operations. |

`fix` works in place, so by default it refuses to run on a dirty working
tree: `git diff` then shows exactly what Reverse Codemosis did, and
`git checkout .` undoes it.

## Built-in packs

| Pack | Language | Rules |
|---|---|---|
| [`python-core`](packs/python-core) | Python | 17: missing imports, undefined names, `is` with literals, vacuous asserts, duplicate keys, missing f-prefixes, mutable defaults, bare except, `== None`, unreachable code, unused imports, redundant code, boolean returns |
| [`python-legacy`](packs/python-legacy) | Python | 6: Python 2 constructs with exact Python 3 equivalents |
| [`rust-core`](packs/rust-core) | Rust | 5: needless `return`, field-init shorthand, `-> ()`, manual `assert!`, `todo!()` left in code |

## Project status

This is the foundation: the engine, the pack format and its proof system,
the verification loop, three packs and the demonstration. See
[docs/ROADMAP.md](docs/ROADMAP.md) for what comes next, starting with more
languages and compiler diagnostics as a source of facts.

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
the same terms.
