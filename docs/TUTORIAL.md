# Writing your first rule

This walk-through builds a rule from nothing to a published, proven,
documented pack. It takes about twenty minutes. The rule is small on
purpose; the method is the point: find the shape, state the facts that make
the correction safe, and prove it with examples that would catch a wrong
rule.

We will correct `if (!!x)` to `if (x)` in JavaScript and TypeScript.

## 1. Start a pack

```console
$ codemosis pack new my-js-pack --id my-js-pack --language javascript
$ codemosis pack test my-js-pack
```

`pack new` writes a manifest and one working rule for the language you
name (every grammar has a template; `--language "*"` starts a text pack
that works on any language). The JavaScript template happens to be the rule
this tutorial builds, so you can compare as you go. Delete
`rules/double-bang-condition.toml` if you want to type it yourself.

## 2. Find the shape

Write the code you want to match in a file and look at its syntax tree:

```console
$ printf 'if (!!user.active) {\n  go();\n}\n' > t.mjs
$ codemosis tree t.mjs
program  [1:1]
  if_statement  [1:1]
    "if"  [1:1]
    condition: parenthesized_expression  [1:4]
      "("  [1:4]
      unary_expression  [1:5]
        operator: "!"  [1:5]
        argument: unary_expression  [1:6]
          operator: "!"  [1:6]
          argument: member_expression  [1:7]
...
```

A shape is a [tree-sitter query](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/1-syntax.html)
over that tree. Name the parts you will talk about with captures:

```toml
shape = """
(if_statement condition: (parenthesized_expression
  (unary_expression operator: "!" argument: (unary_expression operator: "!" argument: (_) @value))) @cond)
"""
```

## 3. Decide whether the correction is always right

This is the step that matters. `!!x` converts `x` to a boolean. An `if`
condition converts its value to a boolean anyway, so `if (!!x)` and `if (x)`
take the same branch for **every** `x`: objects, `NaN`, `0`, `""`,
`document.all`, anything. The correction preserves behaviour.

Outside a condition it does not: `const ready = !!flag` stores `true` or
`false`, while `const ready = flag` stores `flag` itself. The shape already
excludes that case, because it only matches the condition of an `if`.

When a correction is right only under some condition, write that condition
as a fact in `where` (`codemosis facts` lists the vocabulary). When no fact
can establish it, the rule must ask (`[ask]`) or only explain. Never encode
"usually".

## 4. Write the rule

```toml
id = "double-bang-condition"
title = "Double negation in a condition"
category = "simplification"
semantics = "preserved"
summary = "`${cond}` converts to a boolean that the condition converts again; `(${value})` means the same."
explain = """
`!!x` turns a value into true or false. An `if` condition already tests the
value's truthiness, so `if (!!x)` and `if (x)` take the same branch for every
x. Outside a condition `!!x` changes the value, so the rule only matches
conditions.
"""
shape = """..."""   # from step 2

[fix]
edits = [{ replace = "@cond", with = "(${value})" }]
```

`semantics = "preserved"` is a claim the self-test will check by running the
examples before and after the correction and comparing their output.

## 5. Prove it

An example must turn into exactly its `after`, and, because the claim is
`preserved`, print the same thing before and after. Choose values that would
expose a wrong rule: every falsy value JavaScript has.

```toml
[[example]]
name = "truthy and falsy values"
before = '''
for (const x of [0, 1, "", "a", null, [], NaN]) {
  if (!!x) { console.log("yes", x); } else { console.log("no", x); }
}
'''
after = '''
for (const x of [0, 1, "", "a", null, [], NaN]) {
  if (x) { console.log("yes", x); } else { console.log("no", x); }
}
'''

[[counterexample]]
name = "a boolean conversion that is stored"
code = '''
const ready = !!process.env.READY;
console.log(ready);
'''
```

```console
$ codemosis pack test my-js-pack -v
my-js-pack 0.1.0 (my-js-pack)
  ✓ my-js-pack/double-bang-condition (5 checks)
      ok truthy and falsy values: rewrites to `after`
      ok truthy and falsy values: `after` has no new syntax errors
      ok truthy and falsy values: nothing left to correct in `after`
      ok truthy and falsy values: behaves as `preserved`
      ok a boolean conversion that is stored: not matched
```

Break the rule on purpose to see the proof work: change the fix to
`with = "${value}"` (dropping the parentheses) and `pack test` fails,
because `if user.active` is not JavaScript.

## 6. Make it strong

```console
$ codemosis pack lint my-js-pack -v          # unused captures, thin explanations...
$ codemosis pack test my-js-pack --mutations # which facts do the counterexamples need?
```

Mutation testing removes each `where` fact in turn and reruns the proofs. A
fact whose removal no check notices is unproven: add the counterexample that
needs it, or delete the fact. Our rule has no facts, because the shape alone
carries the condition.

Optional metadata helps reports and dashboards:

```toml
tags = ["readability"]
references = ["https://developer.mozilla.org/en-US/docs/Glossary/Truthy"]
# cwe = ["CWE-..."]   for rules that address a known weakness
```

## 7. Use it

```console
$ codemosis check --pack ./my-js-pack path/to/project
$ codemosis fix   --pack ./my-js-pack path/to/project
```

or permanently, in the project's `codemosis.toml`:

```toml
[packs]
paths = ["../my-js-pack"]
```

`codemosis lock` then pins the pack by content, so a later change to it is a
reviewed change.

## 8. Document and share it

```console
$ codemosis pack docs my-js-pack --output docs/
```

writes a reference page generated from the rules and their proofs, like the
ones in [docs/packs](packs/README.md). Publish the pack as a git repository;
others add it under `[packs] paths` and pin it with `codemosis lock`.

To contribute a rule to a built-in pack instead, see
[CONTRIBUTING.md](../CONTRIBUTING.md). The full format is in
[PACK_FORMAT.md](PACK_FORMAT.md).
