# ocaml-core 0.1.0

> Explanations for OCaml: Obj.magic.

**Languages:** ocaml · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`obj-magic`](#obj-magic) Obj.magic | hazard | explains | 1 ex. · 1 counter |

## obj-magic

**Obj.magic** · `ocaml-core/obj-magic`

category: hazard · action: explains · tags: type-safety

`Obj.magic` converts any value to any type without a check. When the types do not really match, the
program reads memory as the wrong kind of value: wrong results at best, a segmentation fault at worst,
and the type checker can no longer help. Use a variant, a GADT or a proper conversion. Nothing is changed
automatically: the safe form depends on what the cast was for.

**Shape**

```scheme
(value_path (module_path (module_name) @module) (value_name) @name
  (#eq? @module "Obj") (#eq? @name "magic")) @use
```

**Example: 1**

```text
let as_int (x : string) : int = Obj.magic x
```

**Counterexample: 1** (no finding)

```text
let as_int (x : string) : int = int_of_string x
```

