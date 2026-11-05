# Field test

What Reverse Codemosis found, corrected, asked and explained on twenty
well-known open-source projects, what that revealed about the tool, and what
changed as a result. Run on 2026-11-04 and 2026-11-05 against engine 0.1.0;
`scripts/field-test.sh` reruns it at the same commits.

## In one paragraph

On 10,780 files in eleven languages, the final run proposes **127 proven
corrections, all of which were read and are correct**, asks **128
questions** that a careful reviewer would also ask, and explains **9
findings**, including **three genuine bugs** in rich and Django. The first
run was far worse: one file in rich took about 20 minutes, and about 300
questions and 190 corrections were noise. Those were fixed in the engine and
the packs, not by tuning the repositories. One problem remains open and
leads the [roadmap](ROADMAP.md): corrections are not yet checked against
each project's formatter.

## The repositories

| Repository | Language | Files read | Commit |
|---|---|---:|---|
| [psf/requests](https://github.com/psf/requests) | Python | 122 | `3c7baaa` |
| [pallets/flask](https://github.com/pallets/flask) | Python | 231 | `49943b5` |
| [pallets/click](https://github.com/pallets/click) | Python | 176 | `2247b35` |
| [encode/httpx](https://github.com/encode/httpx) | Python | 115 | `b5addb6` |
| [Textualize/rich](https://github.com/Textualize/rich) | Python | 531 | `9d8f9a3` |
| [django/django](https://github.com/django/django) | Python | 5,716 | `a762974` |
| [expressjs/express](https://github.com/expressjs/express) | JavaScript | 216 | `43fca52` |
| [axios/axios](https://github.com/axios/axios) | JavaScript, TypeScript | 513 | `fb88a1b` |
| [chalk/chalk](https://github.com/chalk/chalk) | JavaScript | 33 | `7211001` |
| [vercel/ms](https://github.com/vercel/ms) | TypeScript | 23 | `4ff48ce` |
| [spf13/cobra](https://github.com/spf13/cobra) | Go | 65 | `adbc881` |
| [gin-gonic/gin](https://github.com/gin-gonic/gin) | Go | 130 | `0f09c3a` |
| [BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep) | Rust | 217 | `91f5d22` |
| [google/gson](https://github.com/google/gson) | Java | 314 | `65df6e5` |
| [sinatra/sinatra](https://github.com/sinatra/sinatra) | Ruby | 290 | `cb22afd` |
| [guzzle/guzzle](https://github.com/guzzle/guzzle) | PHP | 183 | `7d9b1f4` |
| [jqlang/jq](https://github.com/jqlang/jq) | C | 431 | `5c001b1` |
| [nvm-sh/nvm](https://github.com/nvm-sh/nvm) | Shell | 528 | `4e8758a` |
| [Alamofire/Alamofire](https://github.com/Alamofire/Alamofire) | Swift | 533 | `6c2892e` |
| [square/okio](https://github.com/square/okio) | Kotlin | 413 | `1926f02` |

## Method

1. `codemosis check` on every repository. `check` executes nothing and
   changes nothing.
2. **Every proven correction was read** and judged: is it right in every
   case, and is the result better code?
3. **Every explanation was checked** against the source, and every
   question rule was sampled for whether a careful reviewer would ask the
   same thing.
4. `codemosis fix --no-verify` on the eight repositories with corrections:
   the corrections were written, `codemosis proof verify` replayed every
   certificate, and every changed file was re-parsed with the language's
   own parser (Python's `ast`, `gofmt`, `rustfmt`). The clones were then
   restored.
5. Performance with `check --timings`, on a 4-core machine.

**Not done here: running each project's own test suite as the oracle.**
That executes third-party code, which this environment's safety policy
blocks, rightly. Doing it in an isolated CI job is the first milestone of
the [roadmap](ROADMAP.md).

## Results

| Repository | Proven corrections | Questions | Explained | Suppressed by the project | Unparsed files | Seconds |
|---|---:|---:|---:|---:|---:|---:|
| requests | 2 | 3 | 4 | 0 | 0 | 0.44 |
| flask | 0 | 5 | 0 | 0 | 0 | 0.34 |
| click | 6 | 0 | 0 | 0 | 0 | 0.68 |
| httpx | 1 | 0 | 0 | 0 | 0 | 0.36 |
| rich | 3 | 16 | 2 | 4 | 0 | 0.85 |
| django | 109 | 49 | 0 | 6 | 2 | 7.4 |
| express | 0 | 0 | 0 | 0 | 0 | 0.41 |
| axios | 0 | 1 | 1 | 1 | 0 | 0.77 |
| chalk | 0 | 0 | 0 | 0 | 0 | 0.28 |
| ms | 0 | 0 | 0 | 0 | 0 | 0.28 |
| cobra | 1 | 0 | 0 | 0 | 0 | 0.12 |
| gin | 1 | 8 | 1 | 0 | 0 | 0.12 |
| ripgrep | 2 | 0 | 0 | 0 | 3 | 0.45 |
| gson | 0 | 0 | 0 | 0 | 0 | 0.22 |
| sinatra | 0 | 0 | 0 | 0 | 0 | 0.16 |
| guzzle | 0 | 0 | 0 | 0 | 0 | 0.31 |
| jq | 2 | 0 | 0 | 0 | 26 | 0.49 |
| nvm | 0 | 15 | 0 | 0 | 2 | 0.38 |
| Alamofire | 0 | 1 | 0 | 0 | 5 | 0.55 |
| okio | 0 | 30 | 1 | 0 | 6 | 0.28 |
| **Total** | **127** | **128** | **9** | **11** | **44** | **15.5** |

Nine of the twenty projects get nothing at all. They are mature,
heavily linted codebases, and the rules are deliberately narrow, so this is
the expected result.

### Proven corrections: 127, all correct

| Rule | Count | Example |
|---|---:|---|
| `python-core/redundant-pass` | 112 | `pass` after a docstring or another statement (Django 103, click 5, jq's docs 2, requests 1, rich 1) |
| `python-core/return-bool-if`, `-negated` | 8 | `if bypass: return True` / `return False` → `return bool(bypass)` (requests) |
| `python-core/fstring-without-placeholders` | 2 | `f"unable to get the total number of bytes..."` (rich) |
| `go-core/bool-literal-comparison` | 2 | `if compCmd.Hidden == false` → `if !compCmd.Hidden` (cobra, gin) |
| `python-core/unreachable-code` | 1 | `super().setUpClass()` after `raise` (Django) |
| `rust-core/needless-return` | 1 | ripgrep |
| `rust-core/manual-assert` | 1 | `if !x { panic!(...) }` → `assert!(x, ...)` (ripgrep) |

All 127 preserve behaviour. Three are correct but read less well than what
a person would write, which the roadmap addresses:

- `return bool((self.nargs != 1 or self.multiple) and value == ())`
  (click): the `bool()` looks redundant but is not, because when the left
  side is falsy the expression returns `self.multiple` itself. Dropping it
  needs a type fact saying `self.multiple` is a `bool`.
- `return not (engine != "InnoDB")` (Django): turning `!=` into `==` is
  not provable in general, because a custom `__ne__` may disagree with
  `__eq__`.
- The ripgrep `assert!` is 81 characters wide; ripgrep's `rustfmt.toml`
  allows 79. The file passed `rustfmt --check` before the correction and
  fails after it, so the correction would fail ripgrep's CI formatting check.
  **Fixed since:** corrections are now held to the project's formatter. By
  default this step is refused ("crates/ignore/src/dir.rs is kept formatted
  with rustfmt, and after the correction rustfmt would rewrite line 208");
  with `[format] mode = "reformat"` rustfmt lays the `assert!` out over four
  lines, `rustfmt --check` passes, and `proof verify` replays the layout
  with rustfmt (or, without it, confirms it changes whitespace only).

`fix --no-verify` applied all of them on the eight repositories (Django: 53
files, 175 lines removed). Every proof certificate verified, and every
changed file parses with its language's own parser.

### Questions: 128

| Rule | Count | Where | Would a careful reviewer ask? |
|---|---:|---|---|
| `python-core/fixed-wait` | 47 | Django and Flask tests, requests tests | yes: fixed sleeps in tests are the classic cause of flaky suites |
| `kotlin-core/fixed-wait` | 28 | okio tests | yes |
| `shell-core/unquoted-rm` | 17 | nvm and okio test scripts | yes (ShellCheck SC2086 flags the same lines) |
| `python-core/none-equality` | 10 | rich tests | yes (pycodestyle E711) |
| `python-core/mutable-default` | 9 | Django tests | yes (flake8-bugbear B006) |
| `go-core/fixed-wait` | 8 | gin tests | yes |
| `python-core/bare-except` | 6 | rich | yes (pycodestyle E722) |
| `python-core/missing-f-prefix` | 1 | Django | yes: a genuine bug (below) |
| others | 2 | | yes |

### Explanations: 9

- **Two genuine bugs in rich.** In `examples/suppress.py:19`, `name` is
  undefined in `f"Hello {name}!"`; it is never reached, because `1 / 0`
  raises first. In `tests/test_syntax.py:451`, `markdown` is undefined in
  the file's `__main__` block.
- **Six committed private keys**, all TLS test fixtures (requests ×4, axios,
  gin). Explaining them is right: GitHub's secret scanning flags the same
  files. A `[[suppress]]` entry with `paths = ["tests/certs/**"]` records
  that they are intentional.
- One empty `catch` in an okio sample.

### Genuine issues found in the projects

| Project | Where | Issue |
|---|---|---|
| rich | `examples/suppress.py:19` | undefined name `name` |
| rich | `tests/test_syntax.py:451` | undefined name `markdown` in `__main__` |
| Django | `tests/template_tests/syntax_tests/test_partials.py:49` | `"{k}={v}"` lacks its `f` prefix, so the helper produces the literal text `{k}={v}` |

None has been reported upstream yet.

## What the field test changed

| | First run | Final run |
|---|---|---|
| Proven corrections | 318 | 127 |
| Questions | 292 | 128 |
| Explanations | 33 | 9 |
| Findings already suppressed by the project's own comments | not recognised | 11 |
| Slowest repository | rich: one file took about 20 minutes | Django, 5,716 files: 7.4 s |

The changes, all in the engine and the packs:

- **Cubic matching.** The duplicate-key rules paired every dictionary entry
  with every other, and tree-sitter evaluates that in cubic time. rich's
  3,600-entry emoji table took about 20 minutes. Both rules now match each
  entry once and use a linear fact, `earlier_same_field`. Every shape now
  runs under a deterministic partial-match limit that reports, rather
  than hangs, on a runaway shape. A regression test keeps 6,000-entry
  containers fast. Regexes in hot paths are compiled once, and Python
  occurrences are indexed, which made `undefined-name` 4× faster on Django.
- **Style churn.** 190 corrections removed the `f` from fragments of
  implicit concatenations that do have placeholders. That is correct, but
  Django writes them that way on purpose, and pyflakes treats the
  concatenation as one f-string. The rule now does too.
- **Readability.** Negated comparisons are parenthesised:
  `not (a != b)`, not `not a != b`.
- **Decisions already made.** Rules declare their equivalents in other
  tools, and the engine honours `# noqa`, `# type: ignore[...]`,
  `eslint-disable...`, `NOLINT`, `//nolint`, `# shellcheck disable`,
  `#[allow(clippy::...)]` and their file-level forms.
- **Purpose.** Files are classified as library, test, example, script,
  docs or benchmark. Console output is asked about only in library code;
  fixed waits are not asked about in examples, docs, benchmarks or
  `__main__` blocks. This removed 123 questions about `console.log`, all in
  tests, examples, scripts and docs.
- **Python 3.12–3.15.** PEP 695 type aliases and type parameters are now
  understood (10 false reports in Django), and `frozendict` is a builtin.
- **Templates.** `missing-f-prefix` recognises gettext arguments, strings
  whose result is `.format`-ed and template variables: 26 questions on click
  and Django became the one genuine bug.
- **Vendored code.** Well-known libraries copied into a project (jQuery,
  typeahead...) and generated `.docset` directories are never edited.
- **Parse damage** no longer spreads to a whole C header through its
  include guard.

## Grammar gaps

44 files contain code a grammar could not parse; no correction is made
there, and `check` lists them.

- **C (jq, 26):** macros used as statements or loops
  (`jv_array_foreach(...)`, X-macros).
- **Kotlin (okio, 6):** multiplatform `actual constructor`.
- **Swift (Alamofire, 5):** the empty-tuple expression `()`.
- **Shell (ripgrep, nvm, okio):** zsh scripts, which Linguist files under
  Shell and which are parsed with the bash grammar.
- **JavaScript (Django, 2):** template-processed and Python-2-era files.

## Rerunning it

```console
$ scripts/field-test.sh                  # clones into target/field-test
$ scripts/field-test.sh /tmp/field       # or anywhere else
```

It prints the results table above; the JSON reports sit next to the clones.
To add a repository, add a line with its name, `owner/repo` and full commit
hash to the script.
