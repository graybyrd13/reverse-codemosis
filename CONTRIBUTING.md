# Contributing

Reverse Codemosis gets stronger with every rule a stranger contributes.
Thank you for being one.

## Ways to help

| Contribution | Effort | Start with |
|---|---|---|
| Report a correction that made code worse | minutes | the "A correction made code worse" issue template: the most serious kind of bug |
| Add a wait idiom or other table entry | minutes | one line in `packs/waits/tables/waits.toml` ([developer guide](docs/DEVELOPING.md#teach-the-engine-a-language)) |
| Add a counterexample that proves a fact | minutes | `cargo run -q -- pack test --mutations` lists the facts no counterexample needs yet |
| Write a rule | an hour | the [tutorial](docs/TUTORIAL.md), then the [pack format](docs/PACK_FORMAT.md) |
| Teach the engine a language | hours to days | the [developer guide](docs/DEVELOPING.md#teach-the-engine-a-language) |
| Add a fact, guard or engine feature | days | the [developer guide](docs/DEVELOPING.md), [architecture](docs/ARCHITECTURE.md), [semantics](docs/SEMANTICS.md) |
| Run it on real code and report what it got wrong or missed | an afternoon | [docs/FIELD_TEST.md](docs/FIELD_TEST.md) shows how |

## The bar for a rule

A rule is accepted when:

- its correction is right in **every** case its facts admit, not most of
  them; a rule that is usually right is a rule that asks;
- it asks wherever intent decides, offering the answers a careful person
  would choose between;
- its examples prove its semantics claim by running, and its
  counterexamples show the cases a careless rule would get wrong;
- `pack lint --deny-warnings` is clean and every fact is needed by a
  counterexample (`pack test --mutations`);
- its explanation would satisfy a careful reviewer.

## The bar for engine code

- The acceptance contract holds ([docs/SEMANTICS.md](docs/SEMANTICS.md)).
  Anything that could let a harmful step through needs a soundness test
  that fails without the change.
- Facts err towards blocking a correction when unsure.
- Output stays deterministic, writes go through `fsguard`, printed source
  goes through `redact`.
- Formats, schemas, fingerprints and the fact vocabulary's guarantees
  change only through an RFC ([GOVERNANCE.md](GOVERNANCE.md)) and the
  [compatibility policy](docs/COMPATIBILITY.md).

## Before you send a change

```console
$ cargo fmt --all
$ cargo clippy --all-targets -- -D warnings
$ cargo test --workspace
$ cargo run -q -- pack lint --deny-warnings     # if packs changed
$ UPDATE_GOLDEN=1 cargo test -p codemosis-cli --test demo   # if generated docs changed; review the diff
$ scripts/field-test.sh --compare scripts/field-test-baseline.json   # if rules or the engine changed
```

The last one checks 20 real projects (about a minute, executing nothing of
theirs). If it reports a new proven correction, read it; if it is right,
update the baseline with `--summary-json scripts/field-test-baseline.json`
in the same pull request, so the reviewer reads it too.

Then open a pull request using the template, and add a line to
[CHANGELOG.md](CHANGELOG.md) for anything users will notice.

## Licensing

By contributing you agree that your contributions are licensed under both
the MIT license and the Apache License 2.0, as the project is.
