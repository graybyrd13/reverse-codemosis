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
`!true` is `false` and `!false` is `true`.
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

pub fn new_pack(dir: &Path, id: &str, language: &str) -> Result<()> {
    if !codemosis_core::pack::valid_id(id) {
        bail!("pack id '{id}' must be lowercase letters, digits and dashes");
    }
    let (rule_file, rule) = match language {
        "python" => ("double-negation.toml", PYTHON_RULE),
        "rust" => ("negated-bool-literal.toml", RUST_RULE),
        other => bail!("no template for language '{other}' (available: python, rust)"),
    };
    if dir.join("pack.toml").exists() {
        bail!("{} already contains a pack", dir.display());
    }
    std::fs::create_dir_all(dir.join("rules")).with_context(|| format!("cannot create {}", dir.display()))?;
    let manifest = format!(
        "[pack]\nid = \"{id}\"\nversion = \"0.1.0\"\nlanguage = \"{language}\"\nsummary = \"Describe what this pack corrects.\"\nauthors = []\nlicense = \"MIT OR Apache-2.0\"\n"
    );
    std::fs::write(dir.join("pack.toml"), manifest)?;
    std::fs::write(dir.join("rules").join(rule_file), rule)?;
    Ok(())
}
