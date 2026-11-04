//! Python: grammar, facts, filters, import placement and a snippet runner.

use super::python_scope::{analyze, PyAnalysis, Role, ScopeKind};
use super::{Language, RunOutcome, RunStatus, Runner};
use crate::eval::{children, err, line_end, EvalCtx, EvalError, FactDoc, Value};
use crate::model::Edit;
use crate::process::{self, ScratchDir};
use crate::project::SourceFile;
use std::time::Duration;
use tree_sitter::Node;

pub struct Python;

/// Names Python provides without an import.
pub const BUILTINS: &[&str] = &[
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "copyright",
    "credits",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "exit",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "license",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "quit",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
    "__import__",
    "__build_class__",
    "__debug__",
    "__doc__",
    "__name__",
    "__file__",
    "__package__",
    "__spec__",
    "__loader__",
    "__builtins__",
    "__path__",
    "__class__",
    "__annotations__",
    "__dict__",
    "__module__",
    "__qualname__",
    "Ellipsis",
    "NotImplemented",
    "BaseException",
    "BaseExceptionGroup",
    "Exception",
    "ExceptionGroup",
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "EncodingWarning",
    "EnvironmentError",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "NotADirectoryError",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "PythonFinalizationError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "WindowsError",
    "ZeroDivisionError",
];

const FACTS: &[FactDoc] = &[
    FactDoc { name: "bound", signature: "py.bound(name) -> bool", doc: "True when anything in the file binds or declares the name (assignment, import, def, class, parameter, loop target, global...)." },
    FactDoc { name: "bindings", signature: "py.bindings(name) -> number", doc: "How many places in the file bind the name." },
    FactDoc { name: "references", signature: "py.references(name) -> number", doc: "How many places in the file read the name (string interpolations included)." },
    FactDoc { name: "mentioned", signature: "py.mentioned(name) -> bool", doc: "True when the name appears as a word inside any string or comment (e.g. __all__, getattr, doctests, type comments)." },
    FactDoc { name: "dynamic_names", signature: "py.dynamic_names() -> bool", doc: "True when the file can create or look up names at run time: star imports, globals(), locals(), vars(), exec(), eval(), __import__(), or a module-level __getattr__." },
    FactDoc { name: "builtin", signature: "py.builtin(name) -> bool", doc: "True when the name is a Python builtin." },
    FactDoc { name: "attributes", signature: "py.attributes(name) -> list", doc: "For every read of the name, the attribute accessed on it (`json.dumps` gives 'dumps'); a read that is not an attribute access gives ''." },
    FactDoc { name: "is_reference", signature: "py.is_reference(nodes) -> bool", doc: "True when the identifier reads a name (rather than binding it, or being an attribute or keyword name)." },
    FactDoc { name: "removal_preserves_scope", signature: "py.removal_preserves_scope(nodes) -> bool", doc: "True when deleting the statements cannot change how any name resolves: they contain no yield, global or nonlocal at their own level, and inside a function every name they bind is also bound elsewhere in that function." },
    FactDoc { name: "first_statement", signature: "py.first_statement(block) -> nodes", doc: "The first statement of a block that is not its docstring (empty when there is none)." },
    FactDoc { name: "is_docstring", signature: "py.is_docstring(nodes) -> bool", doc: "True when the node is (or is the string of) a module, class or function docstring." },
    FactDoc { name: "top_level", signature: "py.top_level(nodes) -> bool", doc: "True when the statement sits directly in the module, not inside a block." },
    FactDoc { name: "imported_elsewhere", signature: "py.imported_elsewhere(name) -> bool", doc: "True when another file of the project imports the name from this module, or reads `module.name`." },
    FactDoc { name: "defined_in_project", signature: "py.defined_in_project(name, module?) -> bool", doc: "True when another file of the project defines the name at module level, or the project has a module or package of that name: importing it from the standard library would then be a guess. Bindings that import the name from `module` itself (`import json`, `from pathlib import Path`) are not counted." },
    FactDoc { name: "string_prefix", signature: "py.string_prefix(string) -> string", doc: "The lower-cased prefix letters of a string literal: '', 'f', 'rb'..." },
    FactDoc { name: "placeholders_bound", signature: "py.placeholders_bound(string) -> bool", doc: "True when a plain string contains at least one {placeholder}, every brace belongs to a well-formed placeholder, and each placeholder's name is bound in a scope visible from the string." },
    FactDoc { name: "yields_bool", signature: "py.yields_bool(expr) -> bool", doc: "True when the expression always evaluates to True or False: `not x`, comparisons using only is / is not / in / not in, boolean literals, and and/or of such expressions." },
];

const FILTERS: &[(&str, &str)] = &[
    (
        "as_bool",
        "The expression if it always yields a bool, otherwise bool(expression).",
    ),
    (
        "negate",
        "The logical negation of the expression, parenthesised as needed.",
    ),
];

impl Language for Python {
    fn id(&self) -> &'static str {
        "python"
    }

    fn prefix(&self) -> &'static str {
        "py"
    }

    fn linguist_names(&self) -> &'static [&'static str] {
        &["Python"]
    }

    fn definition_kinds(&self) -> &'static [(&'static str, &'static str)] {
        &[("function_definition", "name"), ("class_definition", "name")]
    }

    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn example_file(&self) -> &'static str {
        "example.py"
    }

    fn is_comment(&self, node: &Node) -> bool {
        node.kind() == "comment"
    }

    fn definition_name(&self, node: &Node, text: &str) -> Option<String> {
        match node.kind() {
            "function_definition" | "class_definition" => node
                .child_by_field_name("name")
                .map(|n| text[n.byte_range()].to_string()),
            _ => None,
        }
    }

    fn facts(&self) -> &'static [FactDoc] {
        FACTS
    }

    fn filters(&self) -> &'static [(&'static str, &'static str)] {
        FILTERS
    }

    fn call<'a>(&self, name: &str, args: &[Value<'a>], ctx: &EvalCtx<'a>) -> Option<Result<Value<'a>, EvalError>> {
        if !FACTS.iter().any(|f| f.name == name) {
            return None;
        }
        Some(call_fact(name, args, ctx))
    }

    fn filter<'a>(
        &self,
        name: &str,
        _arg: Option<&str>,
        value: &Value<'a>,
        ctx: &EvalCtx<'a>,
    ) -> Option<Result<Value<'a>, EvalError>> {
        let run = || -> Result<Value<'a>, EvalError> {
            let nodes = ctx.nodes(value, name)?;
            let Some(node) = nodes.first() else {
                return err(format!("py.{name}: empty capture"));
            };
            let text = ctx.node_text(node).to_string();
            Ok(Value::Str(match name {
                "as_bool" => as_bool(node, &text),
                "negate" => negate(node, &text, ctx),
                _ => unreachable!(),
            }))
        };
        match name {
            "as_bool" | "negate" => Some(run()),
            _ => None,
        }
    }

    fn add_import(&self, file: &SourceFile, statement: &str) -> Result<Edit, String> {
        add_import(file, statement.trim())
    }

    fn runner(&self) -> Option<&dyn Runner> {
        Some(&PythonRunner)
    }
}

fn analysis<'a>(ctx: &EvalCtx<'a>) -> &'a PyAnalysis {
    let file: &'a SourceFile = ctx.file;
    file.analysis(analyze)
}

fn one<'a>(args: &[Value<'a>], name: &str) -> Result<(), EvalError> {
    if args.len() != 1 {
        return err(format!("py.{name}() takes 1 argument, given {}", args.len()));
    }
    Ok(())
}

fn call_fact<'a>(name: &str, args: &[Value<'a>], ctx: &EvalCtx<'a>) -> Result<Value<'a>, EvalError> {
    let a = analysis(ctx);
    match name {
        "dynamic_names" => {
            if !args.is_empty() {
                return err("py.dynamic_names() takes no arguments");
            }
            Ok(Value::Bool(a.dynamic || a.star_import))
        }
        "defined_in_project" => {
            if args.is_empty() || args.len() > 2 {
                return err("py.defined_in_project() takes 1 or 2 arguments");
            }
            let n = ctx.string(&args[0], name)?;
            let module = match args.get(1) {
                Some(m) => Some(ctx.string(m, name)?),
                None => None,
            };
            Ok(Value::Bool(defined_in_project(ctx, &n, module.as_deref())))
        }
        "bound" | "bindings" | "references" | "mentioned" | "builtin" | "attributes" | "imported_elsewhere" => {
            one(args, name)?;
            let n = ctx.string(&args[0], name)?;
            Ok(match name {
                "bound" => Value::Bool(a.is_bound(&n)),
                "bindings" => Value::Num(a.bindings_of(&n).count() as i64),
                "references" => Value::Num(a.references_of(&n).count() as i64),
                "mentioned" => Value::Bool(a.text_words.contains(&n)),
                "builtin" => Value::Bool(BUILTINS.contains(&n.as_str())),
                "attributes" => Value::List(
                    a.references_of(&n)
                        .map(|o| Value::Str(o.attr.clone().unwrap_or_default()))
                        .collect(),
                ),
                "imported_elsewhere" => Value::Bool(imported_elsewhere(ctx, &n)),
                _ => unreachable!(),
            })
        }
        _ => {
            one(args, name)?;
            let nodes = ctx.nodes(&args[0], name)?;
            let Some(first) = nodes.first().copied() else {
                return Ok(match name {
                    "first_statement" => Value::Nodes(Vec::new()),
                    "string_prefix" => Value::Str(String::new()),
                    _ => Value::Bool(false),
                });
            };
            let last = *nodes.last().unwrap();
            Ok(match name {
                "is_reference" => Value::Bool(
                    a.occurrences
                        .iter()
                        .any(|o| o.start == first.start_byte() && o.role == Role::Reference),
                ),
                "removal_preserves_scope" => Value::Bool(removal_preserves_scope(a, &nodes, first, last)),
                "first_statement" => Value::Nodes(first_statement(&first, ctx).into_iter().collect()),
                "is_docstring" => Value::Bool(is_docstring(&first)),
                "top_level" => Value::Bool(first.parent().is_some_and(|p| p.kind() == "module")),
                "string_prefix" => Value::Str(string_prefix(&first, ctx)),
                "placeholders_bound" => Value::Bool(placeholders_bound(a, &first, ctx)),
                "yields_bool" => Value::Bool(yields_bool(&first)),
                _ => return err(format!("unknown fact 'py.{name}'")),
            })
        }
    }
}

const NESTED_SCOPES: &[&str] = &["function_definition", "lambda", "class_definition"];

fn removal_preserves_scope(a: &PyAnalysis, nodes: &[Node], first: Node, last: Node) -> bool {
    // Statements that change the meaning of the whole scope, wherever they are.
    let mut stack: Vec<Node> = nodes.to_vec();
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "yield" | "global_statement" | "nonlocal_statement") {
            return false;
        }
        if NESTED_SCOPES.contains(&n.kind()) {
            continue;
        }
        let mut cursor = n.walk();
        stack.extend(n.children(&mut cursor));
    }
    let (start, end) = (first.start_byte(), last.end_byte());
    let scope = a.scope_at(start, end);
    let s = &a.scopes[scope];
    if s.kind != ScopeKind::Function {
        // Module and class bodies resolve names at run time: an assignment
        // that never runs has no effect on them.
        return true;
    }
    let inside = |o: &super::python_scope::Occurrence| o.start >= start && o.end <= end;
    for o in a
        .occurrences
        .iter()
        .filter(|o| o.role == Role::Binding && o.scope == scope && inside(o))
    {
        if s.globals.contains(&o.name) || s.nonlocals.contains(&o.name) {
            continue;
        }
        let bound_elsewhere = a
            .occurrences
            .iter()
            .any(|p| p.role == Role::Binding && p.scope == scope && p.name == o.name && !inside(p));
        if !bound_elsewhere {
            // The name is local to the function only because of the dead code;
            // removing it would make reads elsewhere resolve to a global.
            return false;
        }
    }
    true
}

fn is_docstring(node: &Node) -> bool {
    let stmt = if node.kind() == "string" {
        match node.parent() {
            Some(p) if p.kind() == "expression_statement" => p,
            _ => return false,
        }
    } else {
        *node
    };
    if stmt.kind() != "expression_statement" {
        return false;
    }
    let kids = children(&stmt);
    if kids.len() != 1 || kids[0].kind() != "string" {
        return false;
    }
    let Some(parent) = stmt.parent() else { return false };
    let is_body = parent.kind() == "module"
        || (parent.kind() == "block"
            && parent
                .parent()
                .is_some_and(|g| matches!(g.kind(), "function_definition" | "class_definition")));
    is_body && children(&parent).first().map(|f| f.id()) == Some(stmt.id())
}

fn first_statement<'a>(block: &Node<'a>, _ctx: &EvalCtx<'a>) -> Option<Node<'a>> {
    children(block).into_iter().find(|c| !is_docstring(c))
}

fn string_prefix(node: &Node, ctx: &EvalCtx<'_>) -> String {
    let text = ctx.node_text(node);
    let quote = text.find(['"', '\'']).unwrap_or(0);
    text[..quote].to_lowercase()
}

/// The text between a string literal's quotes.
fn string_body<'a>(node: &Node<'a>, ctx: &EvalCtx<'a>) -> Option<&'a str> {
    let mut cursor = node.walk();
    let kids: Vec<Node> = node.children(&mut cursor).collect();
    let start = kids.iter().find(|k| k.kind() == "string_start")?;
    let end = kids.iter().rev().find(|k| k.kind() == "string_end")?;
    Some(&ctx.file.text[start.end_byte()..end.start_byte()])
}

fn placeholders_bound(a: &PyAnalysis, node: &Node, ctx: &EvalCtx<'_>) -> bool {
    if node.kind() != "string" {
        return false;
    }
    let prefix = string_prefix(node, ctx);
    if prefix.contains('f') || prefix.contains('b') || prefix.contains('t') {
        return false;
    }
    let Some(body) = string_body(node, ctx) else {
        return false;
    };
    let re =
        regex::Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)(\.[A-Za-z_][A-Za-z0-9_]*)*(\[[^\[\]{}]*\])?(![rsa])?(:[^{}]*)?$")
            .expect("valid regex");
    let scope = a.scope_at(node.start_byte(), node.end_byte());
    let mut found = 0;
    let mut rest = body;
    loop {
        let open = rest.find('{');
        let close = rest.find('}');
        match (open, close) {
            (None, None) => break,
            (None, Some(_)) => return false,
            (Some(o), Some(c)) if c < o => return false,
            (Some(o), _) => {
                let after = &rest[o + 1..];
                let Some(end) = after.find('}') else { return false };
                let inner = &after[..end];
                let Some(caps) = re.captures(inner) else { return false };
                let name = caps.get(1).unwrap().as_str();
                if !a.visible_from(scope, name) {
                    return false;
                }
                found += 1;
                rest = &after[end + 1..];
            }
        }
    }
    found > 0
}

fn yields_bool(node: &Node) -> bool {
    match node.kind() {
        "not_operator" | "true" | "false" => true,
        "parenthesized_expression" => children(node).first().is_some_and(|n| yields_bool(n)),
        "boolean_operator" => {
            let l = node.child_by_field_name("left");
            let r = node.child_by_field_name("right");
            matches!((l, r), (Some(l), Some(r)) if yields_bool(&l) && yields_bool(&r))
        }
        "comparison_operator" => {
            let mut cursor = node.walk();
            let ops: Vec<Node> = node.children_by_field_name("operators", &mut cursor).collect();
            !ops.is_empty()
                && ops
                    .iter()
                    .all(|o| matches!(o.kind(), "in" | "not in" | "is" | "is not"))
        }
        _ => false,
    }
}

fn as_bool(node: &Node, text: &str) -> String {
    if yields_bool(node) {
        text.to_string()
    } else {
        format!("bool({})", crate::template::strip_parens(text))
    }
}

fn negate(node: &Node, text: &str, ctx: &EvalCtx<'_>) -> String {
    if node.kind() == "not_operator" {
        if let Some(arg) = node.child_by_field_name("argument") {
            return as_bool(&arg, ctx.node_text(&arg));
        }
    }
    if node.kind() == "parenthesized_expression" {
        if let Some(inner) = children(node).first() {
            return negate(inner, ctx.node_text(inner), ctx);
        }
    }
    // `not (a in b)` is defined to be `a not in b`, and likewise for `is`.
    if node.kind() == "comparison_operator" {
        let mut cursor = node.walk();
        let ops: Vec<Node> = node.children_by_field_name("operators", &mut cursor).collect();
        if ops.len() == 1 {
            let flipped = match ops[0].kind() {
                "in" => Some("not in"),
                "not in" => Some("in"),
                "is" => Some("is not"),
                "is not" => Some("is"),
                _ => None,
            };
            if let Some(f) = flipped {
                let t = &ctx.file.text;
                return format!(
                    "{}{}{}",
                    &t[node.start_byte()..ops[0].start_byte()],
                    f,
                    &t[ops[0].end_byte()..node.end_byte()]
                );
            }
        }
    }
    match node.kind() {
        "boolean_operator" | "conditional_expression" | "lambda" | "named_expression" => {
            format!("not ({text})")
        }
        _ => format!("not {text}"),
    }
}

fn imported_elsewhere(ctx: &EvalCtx<'_>, name: &str) -> bool {
    let path = &ctx.file.path;
    let stem = if ctx.file.file_name() == "__init__.py" {
        path.parent()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    if stem.is_empty() {
        return true;
    }
    let dotted = format!("{stem}.{name}");
    for other in &ctx.project.files {
        if other.path == *path || other.lang.map(|l| l.id()) != Some("python") {
            continue;
        }
        if other.text.contains(&dotted) {
            return true;
        }
        let Some(other_root) = other.root() else { continue };
        let mut stack = vec![other_root];
        while let Some(n) = stack.pop() {
            if n.kind() == "import_from_statement" {
                let module = n
                    .child_by_field_name("module_name")
                    .map(|m| other.text[m.byte_range()].trim_start_matches('.').to_string())
                    .unwrap_or_default();
                let last = module.rsplit('.').next().unwrap_or("");
                if last == stem || module.is_empty() {
                    let mut cursor = n.walk();
                    for imported in n.children_by_field_name("name", &mut cursor) {
                        let target = if imported.kind() == "aliased_import" {
                            imported.child_by_field_name("name")
                        } else {
                            Some(imported)
                        };
                        if target.is_some_and(|t| other.text[t.byte_range()] == *name) {
                            return true;
                        }
                    }
                }
                continue;
            }
            let mut cursor = n.walk();
            stack.extend(n.children(&mut cursor));
        }
    }
    false
}

/// True when the binding at `start..end` imports `name` from `module`:
/// `import module` (when name == module) or `from module import name`.
fn imports_from(file: &SourceFile, start: usize, end: usize, name: &str, module: &str) -> bool {
    let Some(node) = file.root().and_then(|r| r.descendant_for_byte_range(start, end)) else {
        return false;
    };
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            "dotted_name" | "aliased_import" => cur = n.parent(),
            "import_statement" => {
                // Only a plain, unaliased `import module` binds `module` itself.
                let p = node.parent();
                return name == module
                    && p.is_some_and(|d| d.kind() == "dotted_name" && file.text[d.byte_range()] == *module);
            }
            "import_from_statement" => {
                let m = n.child_by_field_name("module_name").map(|m| &file.text[m.byte_range()]);
                let aliased = node.parent().is_some_and(|p| p.kind() == "aliased_import");
                return m == Some(module) && !aliased;
            }
            _ => return false,
        }
    }
    false
}

fn defined_in_project(ctx: &EvalCtx<'_>, name: &str, module: Option<&str>) -> bool {
    for other in &ctx.project.files {
        if other.path == ctx.file.path || other.lang.map(|l| l.id()) != Some("python") {
            continue;
        }
        let stem = other.path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let package = (other.file_name() == "__init__.py")
            .then(|| {
                other
                    .path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
            })
            .flatten();
        if stem.as_deref() == Some(name) || package.as_deref() == Some(name) {
            return true;
        }
        let a: &PyAnalysis = other.analysis(analyze);
        let conflicting = a
            .bindings_of(name)
            .any(|o| o.scope == 0 && !module.is_some_and(|m| imports_from(other, o.start, o.end, name, m)));
        if conflicting {
            return true;
        }
    }
    false
}

/// Top-level standard-library modules, used only to place new imports in
/// the right group. A module missing here is placed as a third-party import,
/// which changes layout, never meaning.
const STDLIB: &[&str] = &[
    "__future__",
    "abc",
    "argparse",
    "array",
    "ast",
    "asyncio",
    "atexit",
    "base64",
    "bisect",
    "builtins",
    "bz2",
    "calendar",
    "cmath",
    "codecs",
    "collections",
    "concurrent",
    "configparser",
    "contextlib",
    "contextvars",
    "copy",
    "csv",
    "ctypes",
    "dataclasses",
    "datetime",
    "decimal",
    "difflib",
    "dis",
    "email",
    "enum",
    "errno",
    "faulthandler",
    "fnmatch",
    "fractions",
    "functools",
    "gc",
    "getpass",
    "gettext",
    "glob",
    "graphlib",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "keyword",
    "locale",
    "logging",
    "lzma",
    "math",
    "mimetypes",
    "multiprocessing",
    "numbers",
    "operator",
    "os",
    "pathlib",
    "pickle",
    "platform",
    "pprint",
    "queue",
    "random",
    "re",
    "secrets",
    "select",
    "selectors",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "socket",
    "sqlite3",
    "ssl",
    "stat",
    "statistics",
    "string",
    "struct",
    "subprocess",
    "sys",
    "sysconfig",
    "tarfile",
    "tempfile",
    "textwrap",
    "threading",
    "time",
    "timeit",
    "tomllib",
    "traceback",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uuid",
    "warnings",
    "weakref",
    "xml",
    "zipfile",
    "zlib",
    "zoneinfo",
];

const IMPORT_KINDS: &[&str] = &["import_statement", "import_from_statement", "future_import_statement"];

fn is_relative(node: &Node) -> bool {
    node.child_by_field_name("module_name")
        .is_some_and(|m| m.kind() == "relative_import")
}

fn add_import(file: &SourceFile, statement: &str) -> Result<Edit, String> {
    let text = &file.text;
    let root = file.root().ok_or("the file has no syntax tree")?;
    let top: Vec<Node> = children(&root);

    // `from m import n`: join an existing single-line import from m.
    if let Some(rest) = statement.strip_prefix("from ") {
        let (module, name) = rest
            .split_once(" import ")
            .ok_or_else(|| format!("cannot read import statement '{statement}'"))?;
        let (module, name) = (module.trim(), name.trim());
        for stmt in top.iter().filter(|s| s.kind() == "import_from_statement") {
            let m = stmt.child_by_field_name("module_name").map(|m| &text[m.byte_range()]);
            let stmt_text = &text[stmt.byte_range()];
            if m != Some(module) || stmt_text.contains('(') || stmt_text.contains('*') || stmt_text.contains('\n') {
                continue;
            }
            let mut cursor = stmt.walk();
            let names: Vec<Node> = stmt.children_by_field_name("name", &mut cursor).collect();
            if names.iter().any(|n| &text[n.byte_range()] == name) {
                return Err(format!("`{name}` is already imported from {module}"));
            }
            let last = names.last().ok_or("empty import")?;
            return Ok(Edit {
                start: last.end_byte(),
                end: last.end_byte(),
                text: format!(", {name}"),
            });
        }
    } else if !statement.starts_with("import ") {
        return Err(format!("'{statement}' is not an import statement"));
    }

    // Otherwise add a line to the leading import block. A standard-library
    // import joins the standard-library group in alphabetical position; any
    // other import goes after the last absolute import, before relative ones.
    let new_module = statement
        .strip_prefix("from ")
        .or_else(|| statement.strip_prefix("import "))
        .and_then(|r| r.split_whitespace().next())
        .unwrap_or("")
        .to_string();
    let is_from = statement.starts_with("from ");
    let mut docstring: Option<Node> = None;
    let mut block: Vec<Node> = Vec::new();
    for (i, stmt) in top.iter().enumerate() {
        if i == 0 && is_docstring(stmt) {
            docstring = Some(*stmt);
            continue;
        }
        if !IMPORT_KINDS.contains(&stmt.kind()) {
            break;
        }
        block.push(*stmt);
    }
    let module_of = |stmt: &Node| -> String {
        let n = if stmt.kind() == "import_statement" {
            stmt.child_by_field_name("name").map(|n| match n.kind() {
                "aliased_import" => n.child_by_field_name("name").unwrap_or(n),
                _ => n,
            })
        } else {
            stmt.child_by_field_name("module_name")
        };
        n.map(|n| text[n.byte_range()].to_string()).unwrap_or_default()
    };
    let is_std = |module: &str| STDLIB.contains(&module.split('.').next().unwrap_or(""));
    let line = format!("{statement}\n");
    let before = |stmt: &Node| {
        let at = crate::eval::line_start(text, stmt.start_byte());
        Edit {
            start: at,
            end: at,
            text: line.clone(),
        }
    };
    let after = |stmt: &Node| {
        let le = line_end(text, stmt.end_byte());
        let at = (le + 1).min(text.len());
        let prefix = if le == text.len() { "\n" } else { "" };
        Edit {
            start: at,
            end: at,
            text: format!("{prefix}{line}"),
        }
    };
    if is_std(&new_module) {
        let std: Vec<&Node> = block
            .iter()
            .filter(|s| s.kind() != "future_import_statement" && !is_relative(s) && is_std(&module_of(s)))
            .collect();
        if !std.is_empty() {
            let same_form: Vec<&&Node> = std
                .iter()
                .filter(|s| (s.kind() == "import_from_statement") == is_from)
                .collect();
            if let Some(next) = same_form.iter().find(|s| module_of(s) > new_module) {
                return Ok(before(next));
            }
            if let Some(last) = same_form.last() {
                return Ok(after(last));
            }
            // `import x` precedes the `from` imports of its group; a `from`
            // import follows the plain ones.
            return Ok(if is_from {
                after(std.last().unwrap())
            } else {
                before(std[0])
            });
        }
        if let Some(first) = block.iter().find(|s| s.kind() != "future_import_statement") {
            return Ok(before(first));
        }
    } else {
        if let Some(last) = block.iter().rev().find(|s| !is_relative(s)) {
            return Ok(after(last));
        }
        if let Some(first) = block.first() {
            return Ok(before(first));
        }
    }
    // No imports yet: after the docstring, or after leading comment lines,
    // separated from the code by a blank line.
    let at = match docstring {
        Some(doc) => (line_end(text, doc.end_byte()) + 1).min(text.len()),
        None => {
            let mut at = 0;
            for l in text.split_inclusive('\n') {
                if l.trim_start().starts_with('#') {
                    at += l.len();
                } else {
                    break;
                }
            }
            at
        }
    };
    let lead = if at > 0 && docstring.is_some() { "\n" } else { "" };
    let trail = if text[at..].starts_with('\n') || text[at..].is_empty() {
        ""
    } else {
        "\n"
    };
    Ok(Edit {
        start: at,
        end: at,
        text: format!("{lead}{line}{trail}"),
    })
}

/// Runs snippets with the `python3` on PATH (or `$CODEMOSIS_PYTHON`).
pub struct PythonRunner;

pub fn python_command() -> String {
    std::env::var("CODEMOSIS_PYTHON").unwrap_or_else(|_| {
        if cfg!(windows) {
            "python".to_string()
        } else {
            "python3".to_string()
        }
    })
}

impl PythonRunner {
    fn exec(&self, code: &str, compile_only: bool) -> RunOutcome {
        let dir = match ScratchDir::new("py") {
            Ok(d) => d,
            Err(e) => return RunOutcome::unavailable(format!("cannot create a scratch directory: {e}")),
        };
        let file = dir.path.join("snippet.py");
        if let Err(e) = std::fs::write(&file, code) {
            return RunOutcome::unavailable(format!("cannot write snippet: {e}"));
        }
        // Not `-I`: it implies `-E`, which would ignore PYTHONHASHSEED and make
        // set and dict-of-str ordering differ between runs. `-s` skips user
        // site-packages; the scratch directory is the only path entry added.
        let mut cmd = process::snippet_command(python_command());
        cmd.arg("-s").arg("-X").arg("utf8");
        if compile_only {
            cmd.arg("-c")
                .arg("import sys; compile(open(sys.argv[1], encoding='utf-8').read(), sys.argv[1], 'exec')")
                .arg(&file);
        } else {
            cmd.arg(&file);
        }
        cmd.current_dir(&dir.path)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONHASHSEED", "0");
        match process::run(&mut cmd, Duration::from_secs(20)) {
            Err(e) => RunOutcome::unavailable(format!("cannot run {}: {e}", python_command())),
            Ok(r) => {
                let status = if r.timed_out {
                    RunStatus::TimedOut
                } else if r.success() {
                    RunStatus::Ok
                } else if compile_only || r.stderr.contains("SyntaxError") {
                    RunStatus::CompileError
                } else {
                    RunStatus::Failed(r.code)
                };
                RunOutcome {
                    status,
                    stdout: r.stdout,
                    stderr: r.stderr,
                }
            }
        }
    }
}

impl Runner for PythonRunner {
    fn describe(&self) -> String {
        python_command()
    }

    fn compile(&self, code: &str) -> RunOutcome {
        self.exec(code, true)
    }

    fn run(&self, code: &str) -> RunOutcome {
        self.exec(code, false)
    }
}
