# universal 0.1.0

> Problems that look the same in every language: unresolved merge conflicts, committed private keys.

These rules read text, not syntax, so they apply to every file of every
language GitHub knows, including the ones without a grammar or a known
comment syntax. They never touch generated or vendored files.

**Languages:** any text · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`merge-conflict`](#merge-conflict) Unresolved merge conflict | broken | asks, then corrects as chosen (repaired) | 2 ex. · 3 counter |
| [`private-key`](#private-key) Private key committed | hazard | explains | 1 ex. · 1 counter |

## merge-conflict

**Unresolved merge conflict** · `universal/merge-conflict`

category: broken · action: asks, then corrects as chosen (repaired)

Git wrote both sides of a conflicting change into the file, between
`<<<<<<<`, `=======` and `>>>>>>>` markers, and the conflict was never
resolved. In almost every language the markers do not even parse, and where
they do (prose, data) both versions are now present.

Only you know which version is right, so the engine asks: keep the first
version (the one marked with `<<<<<<<`, usually yours), the second, or both
in order. If the markers are meant to be there, as in documentation that
shows a conflict, answer keep. Markers inside strings or comments are not
reported.

**Pattern**

```regex
(?m)^<{7}(?: (?:[^\n]*))?\n(?P<ours>(?:[^\n]*\n)*?)(?:\|{7}[^\n]*\n(?:[^\n]*\n)*?)?={7}\n(?P<theirs>(?:[^\n]*\n)*?)>{7}(?: (?:[^\n]*))?(?:\n|$)
```

**Facts that must hold**

- `kind_at(@match) != 'string'`
- `kind_at(@match) != 'comment'`

**Question:** Which side of this conflict should stay?

- **ours**: keep the first version (between <<<<<<< and =======)
- **theirs**: keep the second version (between ======= and >>>>>>>)
- **both**: keep both versions, the first then the second
- **keep**: this is not a conflict (documentation showing one); leave it

**Example: a Python setting, keeping theirs** (answer: theirs)

```diff
--- a/config.py
+++ b/config.py
@@ -1,7 +1,3 @@
 TIMEOUT = 30
-<<<<<<< HEAD
-RETRIES = 3
-=======
 RETRIES = 5
->>>>>>> feature/retries
 print(TIMEOUT, RETRIES)
```

**Example: prose, keeping both** (answer: both)

```diff
--- a/NOTES.md
+++ b/NOTES.md
@@ -1,6 +1,3 @@
 # Release notes
-<<<<<<< HEAD
 - Faster start-up.
-=======
 - New export format.
->>>>>>> main
```

**Counterexample: a heading underline is not a conflict** (no finding)

```text
Title
=======

Body text.
```

**Counterexample: markers inside a string** (no finding)

```text
EXAMPLE = """
<<<<<<< HEAD
a
=======
b
>>>>>>> main
"""
```

**Counterexample: conflict markers inside a comment, documenting a merge** (no finding)

```text
/*
<<<<<<< HEAD
int x = 1;
=======
int x = 2;
>>>>>>> feature
*/
int y = 0;
```


## private-key

**Private key committed** · `universal/private-key`

category: hazard · action: explains · CWE-321, CWE-798

Anyone who can read the repository, or any copy of it, can use this key. It
must be treated as compromised: revoke or rotate it, remove it from the
repository's history, and load keys from a secret store or the environment
instead. Deleting it from the current version alone does not help, so
nothing is changed automatically.

**Pattern**

```regex
-----BEGIN (?:RSA |EC |DSA |OPENSSH |ENCRYPTED |PGP )?PRIVATE KEY(?: BLOCK)?-----
```

**Example: 1**

```text
-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAexampleexampleexample
-----END RSA PRIVATE KEY-----
```

**Counterexample: a public key** (no finding)

```text
-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA
-----END PUBLIC KEY-----
```

