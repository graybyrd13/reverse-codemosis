# Writing a fix pack

A fix pack is a directory of small formal rules. Each rule says: *when this
shape appears and these facts hold, this correction is safe, and here is the
proof*. Packs are plain TOML; no Rust is needed to write one.

```console
$ codemosis pack new my-pack --id my-pack --language python
$ codemosis pack test my-pack
$ codemosis check --pack my-pack path/to/project
```

## Layout

```text
my-pack/
  pack.toml        # the manifest
  rules/           # one rule per file, named after the rule's id
    double-negation.toml
  tables/          # optional data the rules look up
    modules.toml
```

### `pack.toml`

```toml
[pack]
id = "my-pack"                 # lowercase letters, digits, dashes
version = "0.1.0"
language = "python"            # python | rust
summary = "One line: what this pack corrects."
description = "Optional longer text."
authors = ["Ada <ada@example.com>"]
license = "MIT OR Apache-2.0"
homepage = "https://example.com"   # optional
```

Rule ids are qualified by the pack id: `my-pack/double-negation`.

## A rule

```toml
id = "double-negation"
title = "Double negation"
category = "simplification"
semantics = "preserved"
summary = "`${expr}` negates twice; it is `bool(${value})`."
explain = """
`not not x` is the truth value of `x`, which is exactly `bool(x)`. The rule
applies only when the file does not redefine `bool`.
"""
shape = "(not_operator argument: (not_operator argument: (_) @value)) @expr"
where = ["!py.bound('bool')"]

[fix]
edits = [{ replace = "@expr", with = "bool(${value})" }]

[[example]]
before = '''
print(not not [], not not "x")
'''
after = '''
print(bool([]), bool("x"))
'''

[[counterexample]]
code = '''
print(not [])
'''
```

### Fields

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique within the pack. The file must be `rules/<id>.toml`. |
| `title` | yes | A short name for the kind of problem. |
| `category` | yes | `broken`, `contradiction`, `hazard`, `waste` or `simplification`. |
| `semantics` | with a fix | What the correction does to behaviour: `preserved`, `corrected` or `repaired` (below). |
| `summary` | yes | One sentence shown with each finding. A [template](#templates). |
| `explain` | yes | The plain-language explanation shown by `codemosis explain`. Say what is wrong, why, and why the correction is safe or why there is none. |
| `shape` | yes | A tree-sitter query (below). |
| `where` | no | Facts that must all be true (below). |
| `let` | no | Derived values, evaluated in order, usable as captures. |
| `focus` | no | The capture reported as the finding's location. Default: the capture covering the most text. |
| `anchor` | no | Templates identifying a finding. Matches with equal anchors in one file merge into one finding (for example, one missing import used in many places). |
| `evidence` | no | `[[evidence]] at = "@capture", label = "template"`: extra labelled locations. |
| `fix` | no | The correction: `edits = [...]`. Without it the rule only explains. |
| `ask` | no | `question = "template"`: the correction needs a fact only a human has. Phrase it so that "yes" applies the correction. |
| `references` | no | Links to documentation backing the rule. |
| `example` | yes, at least one | Proof that the rule does what it says (below). |
| `counterexample` | yes, at least one | Proof of where the rule stops. |

## Shapes

A shape is a [tree-sitter query](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html).
`codemosis tree file.py` prints any file's tree with field names and
anonymous tokens, so you can see exactly what to match:

```console
$ codemosis tree example.py
module  [1:1]
  expression_statement  [1:1]
    comparison_operator  [1:1]
      identifier "x"  [1:1]
      operators: "is"  [1:3]
      integer "5"  [1:6]
```

Captures (`@name`) name the nodes the rule talks about. The query
predicates `#eq?`, `#match?` and `#any-of?` work as usual. Giving two nodes
the same capture name makes the capture span from the first to the last.

## Facts

`where` lists boolean expressions that must all be true:

```toml
where = [
  "py.is_reference(@name)",
  "!py.bound(text(@name))",
  "kind(@right) in ['integer', 'string']",
  "subset(py.attributes(text(@name)), table('stdlib_modules', text(@name)))",
]
```

The language has `&&` (`and`), `||` (`or`), `!` (`not`), comparisons
(`==`, `!=`, `<`, `<=`, `>`, `>=`), `in` and `not in`, strings in single or
double quotes, numbers, lists `[...]`, captures `@name` and function calls.
It has no assignment, loops or side effects. A fact must evaluate to true or
false: use `exists(@capture)` rather than relying on truthiness.

The functions are the engine's **fact vocabulary**: generic ones such as
`text`, `kind`, `contains`, `occupies_lines`, `siblings_after`, and each
language's own, prefixed (`py.bound`, `py.removal_preserves_scope`...). The
full list with exact meanings is in [FACTS.md](FACTS.md) and in
`codemosis facts`.

**Facts must be conservative.** When a fact cannot be sure, it must answer in
the direction that blocks the correction. If your rule needs a fact the
vocabulary does not have, propose it as an engine change rather than
approximating it in a pack: the vocabulary is shared, reviewed code.

## Derived values

```toml
[let]
dead = "siblings_after(@jump)"                         # nodes
module = "lookup('stdlib_names', text(@name))"         # a string
replacement = "choose(text(@op) == 'is', '==', '!=')"  # a string
```

Each value can be used as a capture in later `let` entries, in `where`, in
templates and (when it holds nodes) as an edit target.

## Templates

`summary`, `ask.question`, `anchor`, evidence labels and edit text are
templates: `${capture}` inserts the capture's source text,
`${capture|filter|filter:arg}` passes it through filters, `${indent}` is one
indentation unit of the file, and `$$` is a literal `$`.

Generic filters: `trim`, `lower`, `upper`, `line`, `strip_parens`,
`unescape_braces`, `drop_string_prefix:x`, `add_string_prefix:x`. Python adds
`py.as_bool` and `py.negate`.

## Edits

| Operation | Effect |
|---|---|
| `{ replace = "@c", with = "..." }` | Replace the capture's text. |
| `{ delete = "@c" }` | Delete the capture's text. |
| `{ delete_lines = "@c" }` | Delete the whole lines the capture occupies (refused if they hold other code). |
| `{ delete_item = "@c" }` | Delete a comma-separated item with its comma. |
| `{ insert_before = "@c", with = "..." }` | Insert text before the capture. |
| `{ insert_after = "@c", with = "..." }` | Insert text after the capture. |
| `{ insert_lines_before = "@c", with = "..." }` | Insert lines before the capture's line, at its indentation. |
| `{ insert_lines_after = "@c", with = "..." }` | Insert lines after the capture's line, at its indentation. |
| `{ add_import = "import json" }` | Add an import where the language conventionally puts it (joining an existing `from` import when possible). |

The engine guards every correction, whatever the rule says: edits may not
overlap, may not delete a comment, and may not leave a file with new syntax
errors.

## Semantics

Every rule with a fix declares what it does to behaviour, and the self-test
checks the claim by running the examples:

| Semantics | Claim | Checked by running |
|---|---|---|
| `preserved` | Behaviour is identical. | `before` and `after` produce the same output and exit status. |
| `corrected` | Behaviour changes from self-contradictory to the code's evident meaning. | `after` runs, produces the declared `output`, and `before` behaves differently. |
| `repaired` | Code that could not run as written now runs, through the only possible repair. | `before` fails to compile or run; `after` runs. |

See [SEMANTICS.md](SEMANTICS.md) for the precise model.

## Examples and counterexamples

```toml
[[example]]
name = "optional description"
path = "pkg/__init__.py"    # optional; defaults to example.py / example.rs
before = '''...'''
after = '''...'''           # required for rules with a fix, forbidden otherwise
output = "...\n"            # optional (required for `corrected`): stdout of `after`
output_before = "...\n"     # optional: stdout of `before`
run = false                 # optional: do not execute this example

[[counterexample]]
name = "optional description"
code = '''...'''
```

`codemosis pack test` checks, for every example:

1. the rule matches `before` (and, for explain-only rules, reports it);
2. applying the rule until it stops matching turns `before` into exactly
   `after` (questions are treated as answered "yes");
3. `after` has no syntax errors and the rule no longer matches it;
4. running `before` and `after` confirms the declared semantics.

Every counterexample must produce no finding at all. Python examples run with
`python3` (override with `CODEMOSIS_PYTHON`), Rust examples with `rustc`
(`CODEMOSIS_RUSTC`): with a `fn main` they are compiled and run, without one
they are compiled as a library.

Write examples that would catch a wrong rule. The `return-bool-if` example
includes a class whose `>` returns `"yes"`: drop the `bool()` from the
correction and the self-test fails.

## Tables

Rules can look up data in tables (`tables/*.toml`). An array of strings is a
set; a table of strings or string arrays is a map:

```toml
side_effect_free_modules = ["json", "os", "re"]

[stdlib_modules]
json = ["dumps", "loads", "dump", "load", "JSONDecodeError"]
```

Use them with `in_table('name', key)`, `table('name', key)` (a list) and
`lookup('name', key)` (the first string). Every entry is a claim the engine
trusts, so every entry must be exact. Leaving something out is always safe.

## Using a pack

```console
$ codemosis check --pack ./my-pack .
$ codemosis fix --pack ./my-pack .
```

or permanently, in the project's `codemosis.toml`:

```toml
[packs]
paths = ["../my-pack"]
```

## The bar for a rule

A rule is accepted into a shared pack when:

- its correction is right in every case its facts admit, not most cases;
- where intent decides, it asks rather than assumes;
- its examples run and prove its semantics claim;
- its counterexamples show the cases a careless rule would get wrong;
- its `explain` text would satisfy a careful reviewer.
