//! `codemosis pack new`: a new pack with one rule that already passes its
//! self-test, so authors start from something that works.

use anyhow::{bail, Context, Result};
use std::path::Path;

const PYTHON_RULE: &str = r#"id = "double-negation"
title = "Double negation"
category = "simplification"
semantics = "preserved"
summary = "`${expr}` negates twice; it is `bool(${value})`."
explain = """
`not not x` is the truth value of `x`, which is exactly `bool(x)`. The rule
applies only when the file does not redefine `bool`.
"""
# The shape is a tree-sitter query. Run `codemosis tree file.py` to see the
# node names of any code you want to match.
shape = "(not_operator argument: (not_operator argument: (_) @value)) @expr"
# Facts that must all hold before the correction is safe.
# Run `codemosis facts` to see the vocabulary.
where = ["!py.bound('bool')"]

[fix]
edits = [{ replace = "@expr", with = "bool(${value})" }]

# Every rule proves itself: `before` must become exactly `after`, and because
# semantics = "preserved", both must print the same thing when run.
[[example]]
before = '''
print(not not [], not not "x")
'''
after = '''
print(bool([]), bool("x"))
'''

# And shows where it stops.
[[counterexample]]
code = '''
print(not [])
'''
"#;

const RUST_RULE: &str = r#"id = "negated-bool-literal"
title = "Negated boolean literal"
category = "simplification"
semantics = "preserved"
summary = "`${expr}` is the literal `${result}`."
explain = """
`!true` is `false` and `!false` is `true`: negating a literal is a constant
spelled the long way. Writing the constant says the same thing to the
compiler and is easier to read; it usually means a condition was simplified
by hand and the leftover was never cleaned up.
"""
# The shape is a tree-sitter query. Run `codemosis tree file.rs` to see the
# node names of any code you want to match.
shape = '(unary_expression "!" (boolean_literal) @literal) @expr'

[let]
result = "choose(text(@literal) == 'true', 'false', 'true')"

[fix]
edits = [{ replace = "@expr", with = "${result}" }]

# Every rule proves itself: `before` must become exactly `after`, and because
# semantics = "preserved", both must compile and print the same thing.
[[example]]
# (Macro arguments such as println!'s are token trees, not expressions, so
# shapes do not see inside them.)
before = '''
fn main() {
    let (a, b) = (!true, !false);
    println!("{} {}", a, b);
}
'''
after = '''
fn main() {
    let (a, b) = (false, true);
    println!("{} {}", a, b);
}
'''

[[counterexample]]
code = '''
fn flip(b: bool) -> bool {
    !b
}
'''
"#;
const JS_RULE: &str = r#"id = "double-bang-condition"
title = "Double negation in a condition"
category = "simplification"
semantics = "preserved"
summary = "`${cond}` converts to a boolean that the condition converts again; `(${value})` means the same."
explain = """
`!!x` turns a value into true or false. An `if` or `while` condition already
tests the value's truthiness, so `if (!!x)` and `if (x)` take the same branch
for every x. Outside a condition `!!x` changes the value (a boolean instead of
x), so the rule only matches conditions.
"""
# The shape is a tree-sitter query. Run `codemosis tree file.mjs` to see the
# node names of any code you want to match.
shape = """
[
  (if_statement condition: (parenthesized_expression
    (unary_expression operator: "!" argument: (unary_expression operator: "!" argument: (_) @value))) @cond)
  (while_statement condition: (parenthesized_expression
    (unary_expression operator: "!" argument: (unary_expression operator: "!" argument: (_) @value))) @cond)
]
"""

[fix]
edits = [{ replace = "@cond", with = "(${value})" }]

# Every rule proves itself: `before` must become exactly `after`, and because
# semantics = "preserved", both must print the same thing when run with node.
[[example]]
name = "truthy and falsy values"
before = '''
for (const x of [0, 1, "", "a", null, [], NaN]) {
  if (!!x) {
    console.log("yes", x);
  } else {
    console.log("no", x);
  }
}
'''
after = '''
for (const x of [0, 1, "", "a", null, [], NaN]) {
  if (x) {
    console.log("yes", x);
  } else {
    console.log("no", x);
  }
}
'''

# And shows where it stops: here `!!` changes the value that is stored.
[[counterexample]]
name = "a boolean conversion that is stored"
code = '''
const ready = !!process.env.READY;
console.log(ready);
'''
"#;

const TEXT_RULE: &str = r#"id = "fixme-comment"
title = "FIXME left in a comment"
category = "waste"
summary = "A FIXME marks work someone meant to finish before this shipped."
explain = """
A FIXME comment records a known problem. The engine cannot know what the fix
is, so the rule only explains: finish the work, or turn the note into an
issue and link it. Only comments count, never strings, so the rule runs in
every language whose comment syntax the engine knows.
"""
# A text rule: a regular expression instead of a tree-sitter shape. It runs
# on every language whose comments the engine can tell from code.
pattern = '\bFIXME\b'
within = "comment"

[[example]]
path = "deploy.ps1"
before = '''
# FIXME: retry when the registry is down
Invoke-WebRequest $url
'''

[[counterexample]]
path = "deploy.ps1"
code = '''
Write-Host "FIXME is only text here"
'''
"#;

/// An explain-only rule on FIXME comments, for a grammar without a richer
/// template.
fn comment_rule(language: &str) -> Option<String> {
    let (kind, marker, close) = match language {
        "java" | "kotlin" | "rust" => ("line_comment", "//", ""),
        "ruby" | "bash" | "python" | "elixir" | "r" | "hcl" | "powershell" | "nix" => ("comment", "#", ""),
        "julia" => ("line_comment", "#", ""),
        "lua" | "haskell" => ("comment", "--", ""),
        "elm" => ("line_comment", "--", ""),
        "erlang" => ("comment", "%", ""),
        "ocaml" => ("comment", "(*", " *)"),
        "go" | "c" | "cpp" | "csharp" | "php" | "scala" | "swift" | "javascript" | "typescript" | "tsx" | "zig"
        | "dart" | "solidity" => ("comment", "//", ""),
        _ => return None,
    };
    let file = codemosis_core::lang::by_id(language)?.example_file();
    let prelude = if language == "php" { "<?php\n" } else { "" };
    // tree-sitter-powershell does not parse a file that is only a comment.
    let trailer = if language == "powershell" {
        "\nWrite-Output 'ok'"
    } else {
        ""
    };
    let mut rule = String::from(
        r#"id = "fixme-comment"
title = "FIXME left in a comment"
category = "waste"
summary = "A FIXME marks work someone meant to finish before this shipped."
explain = """
A FIXME comment records a known problem. The engine cannot know what the fix
is, so the rule only explains: finish the work, or turn the note into an
issue and link it. Start from here: add facts and a [fix] for a rule that
corrects code (see docs/TUTORIAL.md).
"""
"#,
    );
    rule.push_str(&format!(
        "# Run `codemosis tree {file}` to see the node names of any code to match.\n\
         shape = '(({kind}) @note (#match? @note \"FIXME\"))'\nfocus = \"@note\"\n\n\
         [[example]]\npath = \"{file}\"\nbefore = '''\n{prelude}{marker} FIXME: handle the empty case{close}{trailer}\n'''\n\n\
         [[counterexample]]\npath = \"{file}\"\ncode = '''\n{prelude}{marker} a finished note{close}{trailer}\n'''\n"
    ));
    Some(rule)
}

pub fn new_pack(dir: &Path, id: &str, language: &str) -> Result<()> {
    if !codemosis_core::pack::valid_id(id) {
        bail!("pack id '{id}' must be lowercase letters, digits and dashes");
    }
    let generic;
    let (rule_file, rule) = match language {
        "python" => ("double-negation.toml", PYTHON_RULE),
        "rust" => ("negated-bool-literal.toml", RUST_RULE),
        "javascript" | "typescript" => ("double-bang-condition.toml", JS_RULE),
        "*" => ("fixme-comment.toml", TEXT_RULE),
        other => match comment_rule(other) {
            Some(r) => {
                generic = r;
                ("fixme-comment.toml", generic.as_str())
            }
            None => bail!(
                "no grammar '{other}' in this build; use one of {}, or \"*\" for a text pack",
                codemosis_core::lang::all()
                    .iter()
                    .map(|l| l.id())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
    };
    if dir.join("pack.toml").exists() {
        bail!("{} already contains a pack", dir.display());
    }
    std::fs::create_dir_all(dir.join("rules")).with_context(|| format!("cannot create {}", dir.display()))?;
    let v = codemosis_core::version::engine();
    let manifest = format!(
        "[pack]\nid = \"{id}\"\nversion = \"0.1.0\"\nengine = \"^{}.{}\"\nlanguage = \"{language}\"\n\
         summary = \"Describe what this pack corrects.\"\n\
         description = \"Say what the rules have in common and how they prove themselves.\"\n\
         authors = []\nlicense = \"MIT OR Apache-2.0\"\n",
        v.major, v.minor
    );
    std::fs::write(dir.join("pack.toml"), manifest)?;
    std::fs::write(dir.join("rules").join(rule_file), rule)?;
    Ok(())
}
