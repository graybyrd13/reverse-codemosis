# flaky-checkout

A small JavaScript shop module written the way AI tools write code: it
mostly works, one test fails, and the suite is slow because of "let it
settle" waits. There is no `codemosis.toml`: Reverse Codemosis finds the
test command in `package.json` itself.

```console
$ codemosis fix --interactive
```

asks the questions only you can answer, one at a time (keep this fixed
wait? keep this console output?), records your answers in
`.codemosis/answers.toml`, then applies every correction it can prove plus
the ones you chose, checking `npm test` after each step.

| Where | What | What Reverse Codemosis does |
|---|---|---|
| `src/cart.js` | `item.price === NaN` is always false | corrects it to `Number.isNaN(item.price)`: the failing test passes |
| `src/cart.js` | a `debugger` statement | removes it |
| `src/cart.js` | `if (...) { return true; } else { return false; }` | simplifies it to `return this.items.length === 0;` |
| `src/checkout.js` | code after `throw` | removes it |
| `src/checkout.js`, `test/cart.test.js` | `await new Promise((resolve) => setTimeout(resolve, ...))` | **asks**: keep this fixed wait? |
| `src/checkout.js` | `console.log("charging", ...)` | **asks**: keep this console output? |
| `src/checkout.js` | the key `standard` given twice | **explains**: which rate is right is yours to say |

Answering `remove` to all three questions gives the change in
[`../flaky-checkout.diff`](../flaky-checkout.diff), which the test suite in
`crates/codemosis-cli/tests/demo.rs` checks.
