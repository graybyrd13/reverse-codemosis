//! Languages described by a table: a tree-sitter grammar, the Linguist
//! languages it parses, the node kinds that hold comments, strings,
//! definitions and declarations, and how to compile and run a snippet.
//!
//! Every language here gets the generic facts, plus `<prefix>.defines(name)`:
//! true when anything in the file declares the name (a variable, parameter,
//! function, class, import...). It over-approximates declarations, so a rule
//! that needs a name to be undeclared stays safe.

use super::{Language, RunOutcome, RunStatus, Runner};
use crate::eval::{err, EvalCtx, EvalError, FactDoc, Value};
use crate::process::{self, ScratchDir};
use crate::project::SourceFile;
use std::collections::BTreeSet;
use std::time::Duration;
use tree_sitter::Node;

/// A snippet runner defined by commands. `{file}` and `{dir}` are replaced
/// by the snippet's path and its scratch directory.
pub struct CommandRunner {
    pub file: &'static str,
    /// Checks that a snippet compiles.
    pub check: &'static [&'static str],
    /// Steps that build and run a snippet; the last step's output counts.
    pub run: &'static [&'static [&'static str]],
    /// Text a snippet must contain to be run rather than only checked.
    pub runnable_if: Option<&'static str>,
}

impl CommandRunner {
    fn exec(&self, code: &str, run: bool) -> RunOutcome {
        let dir = match ScratchDir::new("snippet") {
            Ok(d) => d,
            Err(e) => return RunOutcome::unavailable(format!("cannot create a scratch directory: {e}")),
        };
        let file = dir.path.join(self.file);
        if let Err(e) = std::fs::write(&file, code) {
            return RunOutcome::unavailable(format!("cannot write snippet: {e}"));
        }
        let runnable = run && self.runnable_if.is_none_or(|marker| code.contains(marker));
        let steps: Vec<&[&str]> = if runnable { self.run.to_vec() } else { vec![self.check] };
        let mut last = None;
        for (i, step) in steps.iter().enumerate() {
            let args: Vec<String> = step
                .iter()
                .map(|a| {
                    a.replace("{file}", &file.to_string_lossy())
                        .replace("{dir}", &dir.path.to_string_lossy())
                })
                .collect();
            let mut cmd = process::snippet_command(&args[0]);
            cmd.args(&args[1..]).current_dir(&dir.path);
            let limit = if i + 1 == steps.len() && runnable { 30 } else { 180 };
            let r = match process::run(&mut cmd, Duration::from_secs(limit)) {
                Ok(r) => r,
                Err(e) => return RunOutcome::unavailable(format!("cannot run {}: {e}", args[0])),
            };
            let is_last = i + 1 == steps.len();
            if !r.success() && (!is_last || !runnable) {
                return RunOutcome {
                    status: if r.timed_out {
                        RunStatus::TimedOut
                    } else {
                        RunStatus::CompileError
                    },
                    stdout: r.stdout,
                    stderr: r.stderr,
                };
            }
            last = Some(r);
        }
        let r = last.expect("at least one step");
        RunOutcome {
            status: if r.timed_out {
                RunStatus::TimedOut
            } else if r.success() {
                RunStatus::Ok
            } else {
                RunStatus::Failed(r.code)
            },
            stdout: if runnable { r.stdout } else { String::new() },
            stderr: r.stderr,
        }
    }
}

impl Runner for CommandRunner {
    fn describe(&self) -> String {
        // The compiler or interpreter: the first program the steps start.
        self.run.first().and_then(|s| s.first()).unwrap_or(&"?").to_string()
    }

    fn compile(&self, code: &str) -> RunOutcome {
        self.exec(code, false)
    }

    fn run(&self, code: &str) -> RunOutcome {
        self.exec(code, true)
    }
}

/// A language described by data.
pub struct TreeSitterLanguage {
    pub id: &'static str,
    pub prefix: &'static str,
    pub linguist: &'static [&'static str],
    pub grammar: fn() -> tree_sitter::Language,
    pub example_file: &'static str,
    /// (node kind, field holding the name) of definitions. A field of `*`
    /// takes the first name inside the node, for grammars without a name
    /// field. A kind `call@def|defp` is a `call` whose `target` is one of
    /// those words (Elixir's `def greet(name)`), named by the field.
    pub definitions: &'static [(&'static str, &'static str)],
    /// (node kind, field holding the declared names, or "*" for the whole node).
    pub declarations: &'static [(&'static str, &'static str)],
    pub runner: Option<&'static CommandRunner>,
}

const FACTS: &[FactDoc] = &[FactDoc {
    name: "defines",
    signature: "<prefix>.defines(name) -> bool",
    doc: "True when anything in the file declares the name: a variable, constant, parameter, function, class, type or import. Over-approximates, so a rule requiring a name to be undeclared stays safe.",
}];

fn first_identifier<'a>(node: Node<'a>, text: &'a str) -> Option<&'a str> {
    let k = node.kind();
    if k.contains("identifier")
        || k == "name"
        || k == "constant"
        || k == "function_name"
        || k == "value_name"
        || k == "alias"
    {
        return Some(&text[node.byte_range()]);
    }
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        if let Some(n) = first_identifier(c, text) {
            return Some(n);
        }
    }
    None
}

fn identifiers_in(node: Node, text: &str, out: &mut BTreeSet<String>) {
    let k = node.kind();
    if (k.contains("identifier") && !k.starts_with("property") && !k.starts_with("field"))
        || k == "name"
        || k == "constant"
        || k == "variable_name"
        || k == "variable"
    {
        out.insert(text[node.byte_range()].trim_start_matches('$').to_string());
    }
    let mut cursor = node.walk();
    for c in node.named_children(&mut cursor) {
        identifiers_in(c, text, out);
    }
}

/// Every name the file declares, per the language's declaration kinds.
fn declared(lang: &TreeSitterLanguage, file: &SourceFile) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(root) = file.root() else { return out };
    let mut stack = vec![root];
    let mut cursor = root.walk();
    while let Some(n) = stack.pop() {
        for (kind, field) in lang.declarations {
            if n.kind() == *kind {
                if *field == "*" {
                    identifiers_in(n, &file.text, &mut out);
                } else {
                    let mut fc = n.walk();
                    for f in n.children_by_field_name(field, &mut fc) {
                        identifiers_in(f, &file.text, &mut out);
                    }
                }
            }
        }
        stack.extend(n.children(&mut cursor));
    }
    out
}

struct Declared(BTreeSet<String>);

impl TreeSitterLanguage {
    /// The definition entry a node matches: (label, field).
    fn definition_entry<'s>(&self, node: &Node, text: &'s str) -> Option<(&'s str, &'static str)>
    where
        'static: 's,
    {
        for (kind, field) in self.definitions {
            match kind.split_once('@') {
                None if *kind == node.kind() => return Some((kind, field)),
                Some((k, targets)) if k == node.kind() => {
                    let target = node.child_by_field_name("target")?;
                    let word = &text[target.byte_range()];
                    if targets.split('|').any(|t| t == word) {
                        return Some((word, field));
                    }
                }
                _ => {}
            }
        }
        None
    }
}

impl Language for TreeSitterLanguage {
    fn id(&self) -> &'static str {
        self.id
    }

    fn prefix(&self) -> &'static str {
        self.prefix
    }

    fn linguist_names(&self) -> &'static [&'static str] {
        self.linguist
    }

    fn grammar(&self) -> tree_sitter::Language {
        (self.grammar)()
    }

    fn example_file(&self) -> &'static str {
        self.example_file
    }

    fn definition_kinds(&self) -> &'static [(&'static str, &'static str)] {
        self.definitions
    }

    fn definition_name(&self, node: &Node, text: &str) -> Option<String> {
        let (_, field) = self.definition_entry(node, text)?;
        if field == "*" {
            let mut cursor = node.walk();
            return node
                .named_children(&mut cursor)
                .find_map(|c| first_identifier(c, text))
                .map(String::from);
        }
        let target = node.child_by_field_name(field)?;
        // A leaf in the name field is the name, whatever the grammar calls
        // it (Bash's `word`, for instance).
        if target.named_child_count() == 0 {
            return Some(text[target.byte_range()].to_string());
        }
        first_identifier(target, text).map(|s| s.to_string())
    }

    fn definition_label(&self, node: &Node, text: &str) -> Option<String> {
        self.definition_name(node, text)?;
        self.definition_entry(node, text).map(|(label, _)| label.to_string())
    }

    fn facts(&self) -> &'static [FactDoc] {
        FACTS
    }

    fn call<'a>(&self, name: &str, args: &[Value<'a>], ctx: &EvalCtx<'a>) -> Option<Result<Value<'a>, EvalError>> {
        if name != "defines" {
            return None;
        }
        Some((|| {
            if args.len() != 1 {
                return err(format!("{}.defines() takes 1 argument", self.prefix));
            }
            let n = ctx.string(&args[0], "defines")?;
            let file: &'a SourceFile = ctx.file;
            let d = file.analysis(|f| Declared(declared(self, f)));
            Ok(Value::Bool(d.0.contains(&n)))
        })())
    }

    fn runner(&self) -> Option<&dyn Runner> {
        self.runner.map(|r| r as &dyn Runner)
    }
}

static NODE_JS: CommandRunner = CommandRunner {
    file: "snippet.mjs",
    check: &["node", "--check", "{file}"],
    run: &[&["node", "{file}"]],
    runnable_if: None,
};
static NODE_TS: CommandRunner = CommandRunner {
    file: "snippet.mts",
    check: &["node", "--experimental-strip-types", "--no-warnings", "{file}"],
    run: &[&["node", "--experimental-strip-types", "--no-warnings", "{file}"]],
    runnable_if: None,
};
static GO: CommandRunner = CommandRunner {
    file: "snippet.go",
    check: &["go", "vet", "{file}"],
    run: &[&["go", "run", "{file}"]],
    runnable_if: Some("func main"),
};
static JAVA: CommandRunner = CommandRunner {
    file: "Snippet.java",
    check: &["javac", "-d", "{dir}", "{file}"],
    run: &[&["java", "{file}"]],
    runnable_if: Some("static void main"),
};
static C: CommandRunner = CommandRunner {
    file: "snippet.c",
    // An undeclared function is an error, as in GCC 14 and Clang 16, so an
    // example compiles the same with every compiler.
    check: &[
        "cc",
        "-std=c11",
        "-Werror=implicit-function-declaration",
        "-fsyntax-only",
        "{file}",
    ],
    run: &[
        &[
            "cc",
            "-std=c11",
            "-Werror=implicit-function-declaration",
            "-o",
            "{dir}/snippet",
            "{file}",
        ],
        &["{dir}/snippet"],
    ],
    runnable_if: Some("main("),
};
static CPP: CommandRunner = CommandRunner {
    file: "snippet.cpp",
    check: &["c++", "-std=c++17", "-fsyntax-only", "{file}"],
    run: &[
        &["c++", "-std=c++17", "-o", "{dir}/snippet", "{file}"],
        &["{dir}/snippet"],
    ],
    runnable_if: Some("main("),
};
static RUBY: CommandRunner = CommandRunner {
    file: "snippet.rb",
    check: &["ruby", "-c", "{file}"],
    run: &[&["ruby", "{file}"]],
    runnable_if: None,
};
static PHP: CommandRunner = CommandRunner {
    file: "snippet.php",
    check: &["php", "-l", "{file}"],
    run: &[&["php", "{file}"]],
    runnable_if: None,
};
static BASH: CommandRunner = CommandRunner {
    file: "snippet.sh",
    check: &["bash", "-n", "{file}"],
    run: &[&["bash", "{file}"]],
    runnable_if: None,
};
static LUA: CommandRunner = CommandRunner {
    file: "snippet.lua",
    check: &["luac", "-p", "{file}"],
    run: &[&["lua", "{file}"]],
    runnable_if: None,
};
static SWIFT: CommandRunner = CommandRunner {
    file: "snippet.swift",
    check: &["swiftc", "-parse", "{file}"],
    run: &[&["swift", "{file}"]],
    runnable_if: None,
};

const JS_DECLARATIONS: &[(&str, &str)] = &[
    ("variable_declarator", "name"),
    ("function_declaration", "name"),
    ("generator_function_declaration", "name"),
    ("function_expression", "name"),
    ("class_declaration", "name"),
    ("class", "name"),
    ("formal_parameters", "*"),
    ("arrow_function", "parameter"),
    ("catch_clause", "parameter"),
    ("import_clause", "*"),
    ("import_specifier", "*"),
    ("namespace_import", "*"),
    ("assignment_expression", "left"),
    ("for_in_statement", "left"),
    ("abstract_class_declaration", "name"),
    ("enum_declaration", "name"),
    ("interface_declaration", "name"),
    ("type_alias_declaration", "name"),
    ("internal_module", "name"),
];
const JS_DEFINITIONS: &[(&str, &str)] = &[
    ("function_declaration", "name"),
    ("generator_function_declaration", "name"),
    ("class_declaration", "name"),
    ("abstract_class_declaration", "name"),
    ("method_definition", "name"),
    ("interface_declaration", "name"),
    ("type_alias_declaration", "name"),
    ("enum_declaration", "name"),
];
const C_DECLARATIONS: &[(&str, &str)] = &[
    ("declaration", "declarator"),
    ("init_declarator", "declarator"),
    ("parameter_declaration", "declarator"),
    ("function_definition", "declarator"),
    ("field_declaration", "declarator"),
    ("struct_specifier", "name"),
    ("enum_specifier", "name"),
    ("union_specifier", "name"),
    ("enumerator", "name"),
    ("type_definition", "declarator"),
    ("preproc_def", "name"),
    ("preproc_function_def", "name"),
    ("class_specifier", "name"),
    ("namespace_definition", "name"),
    ("alias_declaration", "name"),
    ("using_declaration", "*"),
];
const C_DEFINITIONS: &[(&str, &str)] = &[
    ("function_definition", "declarator"),
    ("struct_specifier", "name"),
    ("class_specifier", "name"),
    ("namespace_definition", "name"),
    ("enum_specifier", "name"),
];

pub static LANGUAGES: &[TreeSitterLanguage] = &[
    TreeSitterLanguage {
        id: "javascript",
        prefix: "js",
        linguist: &["JavaScript"],
        grammar: || tree_sitter_javascript::LANGUAGE.into(),
        example_file: "example.mjs",
        definitions: JS_DEFINITIONS,
        declarations: JS_DECLARATIONS,
        runner: Some(&NODE_JS),
    },
    TreeSitterLanguage {
        id: "typescript",
        prefix: "js",
        linguist: &["TypeScript"],
        grammar: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        example_file: "example.mts",
        definitions: JS_DEFINITIONS,
        declarations: JS_DECLARATIONS,
        runner: Some(&NODE_TS),
    },
    TreeSitterLanguage {
        id: "tsx",
        prefix: "js",
        linguist: &["TSX"],
        grammar: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        example_file: "example.tsx",
        definitions: JS_DEFINITIONS,
        declarations: JS_DECLARATIONS,
        runner: None,
    },
    TreeSitterLanguage {
        id: "go",
        prefix: "go",
        linguist: &["Go"],
        grammar: || tree_sitter_go::LANGUAGE.into(),
        example_file: "example.go",
        definitions: &[
            ("function_declaration", "name"),
            ("method_declaration", "name"),
            ("type_spec", "name"),
        ],
        declarations: &[
            ("function_declaration", "name"),
            ("method_declaration", "name"),
            ("var_spec", "name"),
            ("const_spec", "name"),
            ("short_var_declaration", "left"),
            ("parameter_declaration", "name"),
            ("type_spec", "name"),
            ("import_spec", "name"),
            ("range_clause", "left"),
        ],
        runner: Some(&GO),
    },
    TreeSitterLanguage {
        id: "java",
        prefix: "java",
        linguist: &["Java"],
        grammar: || tree_sitter_java::LANGUAGE.into(),
        example_file: "Example.java",
        definitions: &[
            ("class_declaration", "name"),
            ("interface_declaration", "name"),
            ("enum_declaration", "name"),
            ("record_declaration", "name"),
            ("method_declaration", "name"),
            ("constructor_declaration", "name"),
        ],
        declarations: &[
            ("variable_declarator", "name"),
            ("formal_parameter", "name"),
            ("catch_formal_parameter", "name"),
            ("enhanced_for_statement", "name"),
            ("method_declaration", "name"),
            ("class_declaration", "name"),
            ("interface_declaration", "name"),
            ("enum_declaration", "name"),
            ("record_declaration", "name"),
            ("lambda_expression", "parameters"),
            ("import_declaration", "*"),
        ],
        runner: Some(&JAVA),
    },
    TreeSitterLanguage {
        id: "c",
        prefix: "c",
        linguist: &["C"],
        grammar: || tree_sitter_c::LANGUAGE.into(),
        example_file: "example.c",
        definitions: C_DEFINITIONS,
        declarations: C_DECLARATIONS,
        runner: Some(&C),
    },
    TreeSitterLanguage {
        id: "cpp",
        prefix: "c",
        linguist: &["C++"],
        grammar: || tree_sitter_cpp::LANGUAGE.into(),
        example_file: "example.cpp",
        definitions: C_DEFINITIONS,
        declarations: C_DECLARATIONS,
        runner: Some(&CPP),
    },
    TreeSitterLanguage {
        id: "csharp",
        prefix: "cs",
        linguist: &["C#"],
        grammar: || tree_sitter_c_sharp::LANGUAGE.into(),
        example_file: "Example.cs",
        definitions: &[
            ("class_declaration", "name"),
            ("struct_declaration", "name"),
            ("interface_declaration", "name"),
            ("record_declaration", "name"),
            ("method_declaration", "name"),
            ("constructor_declaration", "name"),
            ("namespace_declaration", "name"),
        ],
        declarations: &[
            ("variable_declarator", "*"),
            ("parameter", "name"),
            ("method_declaration", "name"),
            ("local_function_statement", "name"),
            ("class_declaration", "name"),
            ("foreach_statement", "left"),
            ("catch_declaration", "name"),
            ("using_directive", "*"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "ruby",
        prefix: "rb",
        linguist: &["Ruby"],
        grammar: || tree_sitter_ruby::LANGUAGE.into(),
        example_file: "example.rb",
        definitions: &[
            ("class", "name"),
            ("module", "name"),
            ("method", "name"),
            ("singleton_method", "name"),
        ],
        declarations: &[
            ("assignment", "left"),
            ("method", "name"),
            ("singleton_method", "name"),
            ("class", "name"),
            ("module", "name"),
            ("method_parameters", "*"),
            ("block_parameters", "*"),
            ("lambda_parameters", "*"),
        ],
        runner: Some(&RUBY),
    },
    TreeSitterLanguage {
        id: "php",
        prefix: "php",
        linguist: &["PHP"],
        grammar: || tree_sitter_php::LANGUAGE_PHP.into(),
        example_file: "example.php",
        definitions: &[
            ("class_declaration", "name"),
            ("interface_declaration", "name"),
            ("trait_declaration", "name"),
            ("function_definition", "name"),
            ("method_declaration", "name"),
        ],
        declarations: &[
            ("function_definition", "name"),
            ("method_declaration", "name"),
            ("class_declaration", "name"),
            ("simple_parameter", "name"),
            ("assignment_expression", "left"),
            ("namespace_use_clause", "*"),
            ("const_element", "*"),
        ],
        runner: Some(&PHP),
    },
    TreeSitterLanguage {
        id: "bash",
        prefix: "sh",
        linguist: &["Shell"],
        grammar: || tree_sitter_bash::LANGUAGE.into(),
        example_file: "example.sh",
        definitions: &[("function_definition", "name")],
        declarations: &[
            ("variable_assignment", "name"),
            ("function_definition", "name"),
            ("for_statement", "variable"),
        ],
        runner: Some(&BASH),
    },
    TreeSitterLanguage {
        id: "kotlin",
        prefix: "kt",
        linguist: &["Kotlin"],
        grammar: || tree_sitter_kotlin_ng::LANGUAGE.into(),
        example_file: "example.kt",
        definitions: &[
            ("class_declaration", "name"),
            ("object_declaration", "name"),
            ("function_declaration", "name"),
        ],
        declarations: &[
            ("property_declaration", "*"),
            ("function_declaration", "name"),
            ("class_declaration", "name"),
            ("object_declaration", "name"),
            ("parameter", "*"),
            ("class_parameter", "*"),
            ("import", "*"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "scala",
        prefix: "scala",
        linguist: &["Scala"],
        grammar: || tree_sitter_scala::LANGUAGE.into(),
        example_file: "example.scala",
        definitions: &[
            ("class_definition", "name"),
            ("object_definition", "name"),
            ("trait_definition", "name"),
            ("function_definition", "name"),
        ],
        declarations: &[
            ("val_definition", "pattern"),
            ("var_definition", "pattern"),
            ("function_definition", "name"),
            ("class_definition", "name"),
            ("object_definition", "name"),
            ("parameter", "name"),
            ("import_declaration", "*"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "lua",
        prefix: "lua",
        linguist: &["Lua"],
        grammar: || tree_sitter_lua::LANGUAGE.into(),
        example_file: "example.lua",
        definitions: &[("function_declaration", "name")],
        declarations: &[
            ("variable_declaration", "*"),
            ("function_declaration", "name"),
            ("assignment_statement", "*"),
            ("parameters", "*"),
            ("for_generic_clause", "*"),
            ("for_numeric_clause", "name"),
        ],
        runner: Some(&LUA),
    },
    TreeSitterLanguage {
        id: "swift",
        prefix: "swift",
        linguist: &["Swift"],
        grammar: || tree_sitter_swift::LANGUAGE.into(),
        example_file: "example.swift",
        definitions: &[
            ("class_declaration", "name"),
            ("protocol_declaration", "name"),
            ("function_declaration", "name"),
        ],
        declarations: &[
            ("property_declaration", "*"),
            ("function_declaration", "name"),
            ("class_declaration", "name"),
            ("parameter", "*"),
            ("import_declaration", "*"),
        ],
        runner: Some(&SWIFT),
    },
    TreeSitterLanguage {
        id: "elixir",
        prefix: "ex",
        linguist: &["Elixir"],
        grammar: || tree_sitter_elixir::LANGUAGE.into(),
        example_file: "example.exs",
        definitions: &[
            ("call@defmodule|defprotocol|defimpl", "*"),
            ("call@def|defp|defmacro|defmacrop|defguard|defdelegate", "*"),
        ],
        declarations: &[("binary_operator", "left"), ("arguments", "*")],
        runner: None,
    },
    TreeSitterLanguage {
        id: "haskell",
        prefix: "hs",
        linguist: &["Haskell"],
        grammar: || tree_sitter_haskell::LANGUAGE.into(),
        example_file: "example.hs",
        definitions: &[
            ("function", "name"),
            ("bind", "name"),
            ("data_type", "name"),
            ("class", "name"),
        ],
        declarations: &[
            ("function", "name"),
            ("bind", "name"),
            ("patterns", "*"),
            ("import", "*"),
            ("signature", "name"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "ocaml",
        prefix: "ml",
        linguist: &["OCaml"],
        grammar: || tree_sitter_ocaml::LANGUAGE_OCAML.into(),
        example_file: "example.ml",
        definitions: &[
            ("let_binding", "pattern"),
            ("module_binding", "*"),
            ("type_binding", "*"),
        ],
        declarations: &[("let_binding", "pattern"), ("parameter", "*"), ("open_module", "*")],
        runner: None,
    },
    TreeSitterLanguage {
        id: "zig",
        prefix: "zig",
        linguist: &["Zig"],
        grammar: || tree_sitter_zig::LANGUAGE.into(),
        example_file: "example.zig",
        definitions: &[("function_declaration", "name"), ("variable_declaration", "*")],
        declarations: &[
            ("function_declaration", "name"),
            ("variable_declaration", "*"),
            ("parameter", "*"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "dart",
        prefix: "dart",
        linguist: &["Dart"],
        grammar: || tree_sitter_dart::LANGUAGE.into(),
        example_file: "example.dart",
        definitions: &[
            ("class_definition", "name"),
            ("function_signature", "name"),
            ("method_signature", "*"),
        ],
        declarations: &[
            ("class_definition", "name"),
            ("function_signature", "name"),
            ("initialized_variable_definition", "name"),
            ("static_final_declaration", "*"),
            ("formal_parameter", "*"),
            ("import_or_export", "*"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "r",
        prefix: "r",
        linguist: &["R"],
        grammar: || tree_sitter_r::LANGUAGE.into(),
        example_file: "example.R",
        definitions: &[("binary_operator", "lhs")],
        declarations: &[
            ("binary_operator", "lhs"),
            ("parameter", "name"),
            ("for_statement", "variable"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "julia",
        prefix: "jl",
        linguist: &["Julia"],
        grammar: || tree_sitter_julia::LANGUAGE.into(),
        example_file: "example.jl",
        definitions: &[
            ("function_definition", "*"),
            ("macro_definition", "*"),
            ("struct_definition", "*"),
            ("module_definition", "*"),
        ],
        declarations: &[
            ("assignment", "*"),
            ("function_definition", "*"),
            ("const_statement", "*"),
            ("using_statement", "*"),
            ("import_statement", "*"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "erlang",
        prefix: "erl",
        linguist: &["Erlang"],
        grammar: || tree_sitter_erlang::LANGUAGE.into(),
        example_file: "example.erl",
        definitions: &[("function_clause", "name")],
        declarations: &[("function_clause", "name"), ("match_expr", "lhs"), ("expr_args", "*")],
        runner: None,
    },
    TreeSitterLanguage {
        id: "hcl",
        prefix: "hcl",
        linguist: &["HCL"],
        grammar: || tree_sitter_hcl::LANGUAGE.into(),
        example_file: "example.tf",
        definitions: &[("block", "*")],
        declarations: &[("attribute", "*"), ("block", "*")],
        runner: None,
    },
    TreeSitterLanguage {
        id: "powershell",
        prefix: "ps",
        linguist: &["PowerShell"],
        grammar: || tree_sitter_powershell::LANGUAGE.into(),
        example_file: "example.ps1",
        definitions: &[("function_statement", "*"), ("class_statement", "*")],
        declarations: &[
            ("function_statement", "*"),
            ("assignment_expression", "value"),
            ("param_block", "*"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "nix",
        prefix: "nix",
        linguist: &["Nix"],
        grammar: || tree_sitter_nix::LANGUAGE.into(),
        example_file: "example.nix",
        definitions: &[("binding", "attrpath")],
        declarations: &[("binding", "attrpath"), ("formal", "name"), ("inherit", "*")],
        runner: None,
    },
    TreeSitterLanguage {
        id: "elm",
        prefix: "elm",
        linguist: &["Elm"],
        grammar: || tree_sitter_elm::LANGUAGE.into(),
        example_file: "example.elm",
        definitions: &[
            ("value_declaration", "functionDeclarationLeft"),
            ("type_declaration", "name"),
            ("type_alias_declaration", "name"),
        ],
        declarations: &[
            ("value_declaration", "functionDeclarationLeft"),
            ("import_clause", "*"),
            ("type_declaration", "name"),
        ],
        runner: None,
    },
    TreeSitterLanguage {
        id: "solidity",
        prefix: "sol",
        linguist: &["Solidity"],
        grammar: || tree_sitter_solidity::LANGUAGE.into(),
        example_file: "example.sol",
        definitions: &[
            ("contract_declaration", "name"),
            ("interface_declaration", "name"),
            ("library_declaration", "name"),
            ("function_definition", "name"),
            ("modifier_definition", "name"),
            ("event_definition", "name"),
        ],
        declarations: &[
            ("state_variable_declaration", "name"),
            ("variable_declaration", "name"),
            ("parameter", "name"),
            ("function_definition", "name"),
            ("contract_declaration", "name"),
            ("import_directive", "*"),
        ],
        runner: None,
    },
];
