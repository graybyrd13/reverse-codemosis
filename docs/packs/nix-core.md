# nix-core 0.1.0

> Explanations for Nix: fetches that are not pinned by a hash.

**Languages:** nix · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`unpinned-fetch`](#unpinned-fetch) Fetch without a hash | hazard | explains | 1 ex. · 2 counter |

## unpinned-fetch

**Fetch without a hash** · `nix-core/unpinned-fetch`

category: hazard · action: explains · CWE-494 · tags: reproducibility, supply-chain

Given only a URL, the builtin fetchers accept whatever the server returns, and refetch it when the cache
expires, so two builds of the same commit can differ, and a compromised server changes the build. Pass an
attribute set with the hash (`sha256` or `hash`, and `rev` for git), or use the fetchers in nixpkgs, which
require one. Nothing is changed automatically: the hash must come from the content you trust.

**Shape**

```scheme
(apply_expression
  function: [(variable_expression) (select_expression)] @function
  argument: (string_expression)) @call
```

**Facts that must hold**

- `text(@function) in ['fetchTarball', 'builtins.fetchTarball', 'fetchGit', 'builtins.fetchGit', 'builtins.fetchurl']`

**Example: 1**

```text
let
  nixpkgs = builtins.fetchTarball "https://github.com/NixOS/nixpkgs/archive/nixos-24.05.tar.gz";
in
import nixpkgs { }
```

**Counterexample: pinned by a hash** (no finding)

```text
let
  nixpkgs = builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/0123abc.tar.gz";
    sha256 = "0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  };
in
import nixpkgs { }
```

**Counterexample: another function of a string** (no finding)

```text
let
  name = builtins.toString "hello";
in
name
```

