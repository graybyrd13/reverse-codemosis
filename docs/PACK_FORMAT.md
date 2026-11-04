# Writing a fix pack

A fix pack is a directory of small formal rules. Each rule says: *when this
shape appears and these facts hold, this correction is safe, and here is the
proof*. Packs are plain TOML; no Rust is needed to write one.

```console
$ codemosis pack new my-pack --id my-pack --language python   # any grammar, or "*"
$ codemosis pack test my-pack            # run every proof
$ codemosis pack lint my-pack            # find weak spots
$ codemosis pack test my-pack --mutations
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
language = "python"            # a grammar id, or "*" for rules that read any text
# languages = ["javascript", "typescript", "tsx"]   # or several grammars
summary = "One line: what this pack corrects."
description = "Optional longer text."
authors = ["Ada <ada@example.com>"]
license = "MIT OR Apache-2.0"
homepage = "https://example.com"   # optional
engine = "^0.1"                # the engine versions this pack works with
# deprecated = "superseded by better-pack"   # optional
```

`engine` takes comparators such as `^0.1`, `>=0.1, <0.3` or `=0.2.1`. An
engine outside the range refuses the pack with a message instead of
misreading it.

Rule ids are qualified by the pack id: `my-pack/double-negation`.

The grammar ids in this build are `python`, `rust`, `javascript`,
`typescript`, `tsx`, `go`, `java`, `c`, `cpp`, `csharp`, `ruby`, `php`,
`bash`, `kotlin`, `scala`, `lua` and `swift` (`codemosis languages
--deep` lists them). A pack with `language = "*"` holds text rules (below)
and applies to every file whose language the engine can read; it never edits
a file it identified only by guesswork, a generated file or a vendored one.

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
| `shape` | one of three | A tree-sitter query (below). |
| `pattern` | one of three | A regular expression, for [text rules](#text-rules). |
| `patterns` | one of three | The name of a table mapping GitHub language names to regular expressions. |
| `within` | no | Text rules only: where a match must start, `code` (default), `comment`, `string` or `any`. |
| `where` | no | Facts that must all be true (below). |
| `let` | no | Derived values, evaluated in order, usable as captures. |
| `focus` | no | The capture reported as the finding's location. Default: the capture covering the most text. |
| `anchor` | no | Templates identifying a finding. Matches with equal anchors in one file merge into one finding (for example, one missing import used in many places). |
| `evidence` | no | `[[evidence]] at = "@capture", label = "template"`: extra labelled locations. |
| `fix` | no | The correction: `edits = [...]`. Without it the rule only explains. |
| `ask` | no | `question = "template"`: the correction needs a fact only a human has. Phrase it so that "yes" applies the correction, or give named [options](#questions). |
| `references` | no | Links to documentation backing the rule. |
| `tags` | no | Free-form labels (`"timing"`, `"readability"`), shown in reports and SARIF. |
| `cwe` | no | Common Weakness Enumeration ids the rule addresses (`["CWE-396"]`); SARIF consumers use them for security views. |
| `deprecated` | no | Why the rule should no longer be used. It still runs; `explain`, `rules`, SARIF and pack docs say so. |
| `replaced_by` | no | The qualified id of the rule that replaces a deprecated one. |
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

## Text rules

Most of GitHub's 845 languages have no grammar in the engine, and some
problems look the same in all of them. A text rule matches a regular
expression instead of a shape:

```toml
pattern = '(?m)^[ \t]*(?P<call>Start-Sleep\s+\d+)[ \t]*$'
within = "code"
focus = "@match"
where = ["occupies_lines(@match)"]
```

`@match` is the whole match and every named group (`(?P<call>...)`) is a
capture, usable in facts, templates and edits like a node capture. `within`
says where a match must *start*: in `code` (the default), a `comment`, a
`string`, or `any`where. The engine knows comment and string syntax for 343
languages (the lexical tier); a text rule that asks for `code`, `comment` or
`string` never runs on a language whose syntax it does not know, because it
could not tell them apart. `kind_at(@capture)` answers `code`, `comment`,
`string` or `unknown` for finer checks.

`patterns = "table"` names a table of GitHub language name to regular
expressions, so one rule covers many languages and adding a language is one
line of data:

```toml
[wait_patterns]
"PowerShell" = ['(?im)^[ \t]*(?P<call>Start-Sleep\s+[\d.]+)[ \t]*$']
"R" = ['(?m)^[ \t]*(?P<call>Sys\.sleep\(\s*[\d.]+\s*\))[ \t]*$']
```

A pack's tables are checked when the pack loads: every key must be a
language GitHub knows, and every language a `within = "code"` table names
must have a known comment syntax.

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
`unescape_braces`, `bang` (C-style `!x`, parenthesised when needed),
`drop_string_prefix:x`, `add_string_prefix:x`. Python adds `py.as_bool` and
`py.negate`. `codemosis facts` lists every filter.

## Questions

When a correction depends on something only a person knows, the rule asks.
The simplest question is yes or no, phrased so that "yes" applies `[fix]`:

```toml
[ask]
question = "Is `${name}` meant to be the module `${module}`?"
```

When there are more than two reasonable answers, name them. Each option has
a short `means` and its own edits; an option without edits keeps the code
and records that the person chose it:

```toml
[ask]
question = "Keep this fixed wait?"

[[ask.option]]
answer = "keep"
means = "the wait is deliberate (rate limit, back-off, pacing); record it as intentional"

[[ask.option]]
answer = "remove"
means = "delete it; if something must be ready first, wait for that condition instead"
edits = [{ delete_lines = "@match" }]
```

Answers are stored in `.codemosis/answers.toml`, keyed by the finding's
fingerprint, so the same question is never asked twice and `fix` applies
the answer on every run. `codemosis ask` walks through open questions one by
one; `codemosis fix --interactive` asks as it goes; `codemosis answer
<id> <answer>` records one answer from a script.

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
answer = "remove"           # optional: the answer to the rule's question (default "yes",
                            # or the first option with edits)
language = "typescript"     # optional, for packs with several grammars

[[counterexample]]
name = "optional description"
code = '''...'''
language = "tsx"            # optional, as for examples
```

`codemosis pack test` checks, for every example:

1. the rule matches `before` (and, for explain-only rules, reports it);
2. applying the rule until it stops matching turns `before` into exactly
   `after` (questions are treated as answered "yes");
3. `after` has no syntax errors and the rule no longer matches it;
4. running `before` and `after` confirms the declared semantics.

Every counterexample must produce no finding at all.

Examples run with the language's own toolchain: `python3`, `rustc`, `node`
(TypeScript with `--experimental-strip-types`), `go run`, `javac`/`java`,
`cc`, `c++`, `ruby`, `php`, `bash`, `lua` and `swift`, found on `PATH`
(Python and Rust can be overridden with `CODEMOSIS_PYTHON` and
`CODEMOSIS_RUSTC`). When a toolchain is missing the semantic check is
reported as skipped, never as passed. Examples in languages without a runner
(text rules on PowerShell, SQL, Makefiles...) are checked for the match and
the edit only.

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

## Checking a pack

Loading a pack rejects anything that would make a rule wrong or
unrunnable. Three commands go further:

- `codemosis pack test` runs every proof (above).
- `codemosis pack lint` reports what makes a rule weak: captures nobody
  reads (often a forgotten fact; prefix structural ones with `_`), unused
  `let` values and tables, two rules with the same shape, semantics no
  example runs, a `corrected` rule without an expected output, explanations
  under 120 characters, questions with no way to keep the code, missing
  `engine` ranges. `--deny-warnings` makes warnings fail.
- `codemosis pack test --mutations` removes each `where` fact in turn and
  reruns the proofs. A fact whose removal no check notices is unproven: add
  the counterexample that needs it, or remove the fact.
- `codemosis pack docs --output DIR` writes a reference page per pack,
  generated from the rules and their proofs (the built-in ones are in
  [docs/packs](packs/README.md)).

Editors with a TOML language server can validate rules as you type with
`schemas/rule.schema.json` and `schemas/pack.schema.json`.

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

`codemosis lock` pins every pack the project uses by SHA-256 in
`codemosis.lock`; from then on a pack whose files changed does not load
until the change is reviewed and `codemosis lock` is run again. Built-in
packs are pinned too, so an engine upgrade that changes them is a visible,
reviewed change.

## The bar for a rule

A rule is accepted into a shared pack when:

- its correction is right in every case its facts admit, not most cases;
- where intent decides, it asks rather than assumes, and offers the answers a
  careful person would actually choose between;
- its examples run and prove its semantics claim;
- its counterexamples show the cases a careless rule would get wrong;
- its `explain` text would satisfy a careful reviewer.
