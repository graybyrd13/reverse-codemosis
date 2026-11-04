# broken-inventory

A small Python inventory library written the way AI tools write code:
almost right. It does not build and every test fails.

This is the demonstration Reverse Codemosis is built around. One command:

```console
$ codemosis fix
```

and the build passes and all 21 tests are green. The complete change is in
[`../broken-inventory.diff`](../broken-inventory.diff): fourteen corrections
in four files, each one verified by the build and the test suite before it
was kept. The test suite in `crates/codemosis-cli/tests/demo.rs` checks that
the diff stays exactly that.

## What is wrong with it

| Where | What | What Reverse Codemosis does |
|---|---|---|
| `pricing.py` | `except ValueError, error:` (Python 2 syntax) | repairs it: the build passes |
| `store.py` | `Optional` used without an import | imports it from `typing`, the only module that has it |
| `report.py` | `json` and `csv` used without imports | imports them |
| `models.py` | `status is "discontinued"` | corrects to `==` |
| `store.py` | `assert (cond, "message")` that can never fail | makes it assert the condition |
| `store.py`, `pricing.py` | unused imports, unreachable code, a redundant `pass`, `list(sorted(...))`, an f-string with nothing in it | removes the waste |
| `store.py` | `if x in d: return True else: return False` | simplifies to `return x in d` |
| `store.py` | `history=[]` default argument | **asks**: is the sharing intended? |
| `store.py` | bare `except:` | **asks**: should Ctrl-C get through? |
| `report.py` | `"{item.name} ..."` with no `f` | **asks**: did you mean an f-string? |
| `pricing.py` | the key `"bulk"` given twice | **explains**: which value is right is yours to say |

## Run it

From the repository root:

```console
$ scripts/demo.sh
```

The script copies this directory to a temporary git repository, shows the
failing tests, runs `codemosis fix`, shows the passing tests and prints the
diff.

To answer one of the questions, use the id from the report:

```console
$ codemosis answer Q-95adbf57 yes
$ codemosis fix
```

The answer is recorded in `.codemosis/answers.toml`, which you commit like
any other code.
