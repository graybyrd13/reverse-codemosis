# What "proven" means

Reverse Codemosis claims that it never guesses. This document says exactly
what that claim covers, so that it can be checked rather than believed.

## Two sources of truth

A correction is trusted because of two independent checks.

1. **The rule's proof.** A rule's shape and facts define the cases where its
   correction is safe under the language's semantics, and the rule's examples
   demonstrate it by running real code (`codemosis pack test`). The facts are
   written to be *conservative*: whenever one cannot be sure, it blocks the
   correction.
2. **The project's oracle.** On a real project every step is verified by the
   project's own build and tests before it is kept, and rolled back if
   anything that worked before stops working.

Neither alone is enough. The rule's proof covers every input, not only those
the tests exercise, but only within its semantic model. The oracle checks
reality, including what the model leaves out, but only what the tests
exercise.

## The semantic model

A correction marked `preserved` leaves **observable behaviour** unchanged.
Observable behaviour is everything a program does through its inputs and
outputs: values returned, output written, exceptions raised (type and
message), and effects on files, the network and other processes, for every
input.

It deliberately excludes:

- the program's own source text, line numbers and tracebacks;
- introspection of code objects (`f.__code__`, `co_freevars`, bytecode);
- identity of objects that the program creates fresh (whether two new lists
  are the same object), timing, and memory use;
- replacement of builtins or standard-library modules at run time by other
  code (`builtins.sorted = ...`).

The oracle still guards these: if a test depends on one of them (for
example, a test that reads the source file), the step fails verification and
is rolled back. `crates/codemosis-cli/tests/demo.rs` contains exactly that
case.

## The three kinds of correction

| Semantics | Claim |
|---|---|
| `preserved` | Observable behaviour is identical before and after. |
| `corrected` | The code contradicts itself: its behaviour cannot be what it evidently means (an assertion that can never fail, an identity test against a literal). The correction gives it the only meaning it can have had. Behaviour changes, deliberately and only there. |
| `repaired` | The code cannot run as written: it does not compile, or it fails every time the line is reached (a name that is never defined). The repair is the only one possible, for example the only module that provides a name. |

Where more than one meaning or repair is possible, there is no proof, and the
rule must either ask or only explain.

## Questions

Some corrections depend on a fact no analysis can establish: whether a
mutable default is meant to be shared, whether a handler should let Ctrl-C
through, whether a string was meant to be an f-string, whether a fixed wait
is deliberate. The rule then asks. A yes/no question's "yes" supplies the
missing fact and the correction becomes proven *given that fact*; "no"
records the code as intentional, and it is not reported again.

When more than two answers are reasonable, the rule names them: a merge
conflict asks *ours*, *theirs* or *both*; a fixed wait asks *keep* or
*remove*. Each option states what it means and carries its own edits, and
an option without edits records the code as intentional. The answer is the
person's decision; the engine still checks the resulting edit against the
whole acceptance contract, so an answer can never make the code worse
either.

A question that a fixable correction would make moot is not asked: when a
rule can prove that a statement is unreachable and delete it, a question
about a `console.log` inside that statement is dropped, and an explanation
at exactly the place a correction is made is replaced by the correction.

Answers live in `.codemosis/answers.toml`, with the question, the rule and
the location, so a reviewer can see every human decision the corrections
depend on. A question's identity is derived from the rule, the file and the
code it concerns, not from line numbers, so answers survive unrelated edits.

## Knowing what a file is

Before any rule runs, the engine must know a file's language, and it treats
that as a fact like any other. Detection follows GitHub Linguist (filename,
shebang, extension, content heuristics) and reports how sure it is. A file
whose language is only a guess (an ambiguous extension with no evidence
either way) is listed by `codemosis map` but no rule runs on it. Generated
files (lockfiles, minified bundles, files marked `@generated` or "DO NOT
EDIT") and vendored ones (`node_modules/`, `vendor/`, `third_party/`...) are
read but never edited: a correction there would be overwritten or would
diverge from upstream.

Text rules that distinguish code from comments only run on languages whose
comment and string syntax is known. Where it is not, they would have to
guess, so they do not run.

## Conservative facts, by example

`python-core/unreachable-code` deletes statements after a `return`. That
looks trivially safe, but in Python it is not, and the rule's facts encode
why:

- a `yield` anywhere in a function makes it a generator, even in dead code,
  so the rule refuses when the dead code contains one;
- an assignment anywhere in a function makes the name local to the whole
  function, even in dead code. If the dead assignment is the only one,
  deleting it would make earlier reads of the name find a global instead of
  raising `UnboundLocalError`. The rule refuses in that case.

Both cases are among the rule's counterexamples, so the self-test fails if
the rule ever stops handling them.

`python-core/missing-module-import` adds `import json` only when nothing in
the file binds `json`, the file cannot create names at run time (no star
import, `exec`, `eval`, `globals()` or module `__getattr__`), every attribute
read from it exists in the `json` module, and no other file of the project
defines a rival `json`. Then the import is the only repair.

## The acceptance contract

A step on a real project is kept only if:

1. every finding it targets is gone afterwards (a rule that still matches
   its own output is defective, and its step is refused);
2. it introduces no finding of a more serious category;
3. every file it touches still parses, with no new syntax errors;
4. every file it touches that the project's formatter left unchanged before
   is left unchanged by it after (see below);
5. the build does not regress (it does not start failing, or fail with more
   errors);
6. no test that passed before stops passing, is skipped, or disappears.

The engine also refuses any correction that would overlap another or delete
a comment, and three more, whatever a rule says:

- **Binding preservation** (Python, which has a full scope analysis): every
  reference outside the edited text must resolve to the same binding after
  the correction as before. Deleting the only local assignment of a name,
  inserting a definition that shadows a builtin, or removing an import that
  something still reads is refused. A name that resolved to nothing may
  become bound only by a `repaired` rule, which is what adding a missing
  import does. The check runs in every correction step and in every pack
  self-test.
- **Code a build cannot see.** A build checks one configuration, so the
  engine does not edit inside `#if`/`#ifdef` blocks (C, C++, C#,
  Objective-C, Swift; include guards excepted), items behind a Rust
  `#[cfg(...)]` other than `cfg(test)`, or macro bodies, whose token trees
  are not parsed as code. Such a correction could never be verified.
- **Code the grammar could not parse.** No edit touches a construct that
  contains a parse error, because its structure is unknown; `check` lists
  those places. `repaired` rules are the exception: their purpose is to fix
  code that does not parse, and the step must still leave no new error.

### Formatting

Projects that keep their code exactly as a formatter prints it check that in
CI, so a correction that is right but leaves a line too long would still
make the project worse. The engine detects each file's formatter from an
explicit signal (rustfmt from `rustfmt.toml` or `Cargo.toml`, gofmt always
for Go, Black or Ruff from their configuration or pre-commit hooks, Prettier
from its configuration or `package.json`, clang-format from
`.clang-format`). A file is held to its formatter only if the formatter
already leaves it unchanged, so a wrong guess costs nothing. With
`[format] mode = "refuse"` (the default) a step that would leave such a file
unformatted is rolled back with the line the formatter would rewrite. With
`mode = "reformat"` the formatter's output is applied instead, provided
every line it changes lies within three lines of a line the rule changed;
otherwise the step is refused, because it would change code the rule did not
prove. The certificate records the text the rule produced and the formatter
and version that laid it out (below).

Formatting is done over standard input; nothing is written. Prettier can
execute a project's configuration and plugins, so it runs only when the
project's build and tests are the oracle; a missing formatter is reported,
and its files are then not held to it.

Each guard has a case in the soundness regression suite
(`crates/codemosis-engine/tests/soundness.rs`): a rule or input built to
trick the engine, which must be refused.

## Proof certificates

Every `codemosis fix` that changes code writes a certificate to
`.codemosis/proofs/`: the packs (by content hash) and human answers each
step relied on, each step's exact patch, every file's hash before and
after, and the oracle's results. `codemosis proof verify` checks it without
trusting its author: it recovers the original code from the current files
and the patch, replays every step with the pinned rules, and checks that
each reproduces the recorded change byte for byte. When a formatter laid a
step out, the rule's output must match the recorded pre-formatting hash, and
the layout is replayed with the same formatter version when it is
installed; without it, verification accepts the layout only if it leaves
the syntax tree unchanged (whitespace only), and fails otherwise. Verifying
runs none of the project's code, so Prettier is never replayed. A verified
certificate
says the diff is exactly what those reviewed rules prove, given those
recorded answers. What it cannot re-establish is the oracle's verdict; the
recorded test counts are evidence to re-run, not a proof.

## Human decisions about findings

People can also decide that a finding should not be reported: an inline
`codemosis: ignore[rule] reason` comment, or a `[[suppress]]` entry with a
mandatory reason. Both can carry an expiry date (`until=`), after which the
finding comes back with a note. A baseline accepts today's findings so that
only new ones fail a check. None of these hides a finding from the record:
SARIF reports suppressed findings as suppressed, with the justification,
and `check --show-suppressed` lists them with who decided and why.

## What is not claimed

- Reverse Codemosis does not claim that the code is correct afterwards, only
  that every change it made is justified, and that nothing measured got
  worse.
- It does not claim to find every problem. Facts are conservative, so some
  real problems are left alone or only explained.
- A pack's tables are claims about the world (for example, which standard
  library modules have no import side effects). They are reviewed like code,
  and an error in one is a bug in the pack.
