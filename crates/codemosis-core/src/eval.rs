//! Evaluation of fact expressions against a match.
//!
//! The engine provides the vocabulary (the functions below and each
//! language's own facts); packs compose it into rules. Every fact is
//! conservative: when it cannot be sure, it answers in the direction that
//! blocks a correction. A missed fix is acceptable. A wrong fix is not.

use crate::expr::{CmpOp, Expr};
use crate::project::{Project, SourceFile};
use crate::tables::{Table, Tables};
use std::collections::BTreeMap;
use std::fmt;
use tree_sitter::Node;

/// Captures and derived values of one match, by name.
pub type Bindings<'a> = BTreeMap<String, Value<'a>>;

/// A value in the fact language.
#[derive(Clone, Debug)]
pub enum Value<'a> {
    Bool(bool),
    Num(i64),
    Str(String),
    /// Zero or more syntax nodes, in source order.
    Nodes(Vec<Node<'a>>),
    List(Vec<Value<'a>>),
}

impl Value<'_> {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "bool",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::Nodes(_) => "nodes",
            Value::List(_) => "list",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError(pub String);

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EvalError {}

pub fn err<T>(m: impl Into<String>) -> Result<T, EvalError> {
    Err(EvalError(m.into()))
}

/// Everything a fact may look at.
pub struct EvalCtx<'a> {
    pub file: &'a SourceFile,
    pub project: &'a Project,
    pub tables: &'a Tables,
    pub bindings: Bindings<'a>,
}

impl<'a> EvalCtx<'a> {
    pub fn text_of(&self, nodes: &[Node<'a>]) -> String {
        match (nodes.first(), nodes.last()) {
            (Some(first), Some(last)) => {
                self.file.text[first.start_byte()..last.end_byte().max(first.start_byte())].to_string()
            }
            _ => String::new(),
        }
    }

    /// The nodes bound to a name (empty when the name holds another value).
    pub fn bound_nodes(&self, name: &str) -> Vec<Node<'a>> {
        match self.bindings.get(name) {
            Some(Value::Nodes(n)) => n.clone(),
            _ => Vec::new(),
        }
    }

    pub fn node_text(&self, node: &Node<'a>) -> &'a str {
        &self.file.text[node.byte_range()]
    }

    pub fn nodes(&self, v: &Value<'a>, what: &str) -> Result<Vec<Node<'a>>, EvalError> {
        match v {
            Value::Nodes(n) => Ok(n.clone()),
            other => err(format!(
                "{what}: expected nodes (a capture), found {}",
                other.type_name()
            )),
        }
    }

    /// Strings, and nodes (as their source text), are both accepted where
    /// text is expected.
    pub fn string(&self, v: &Value<'a>, what: &str) -> Result<String, EvalError> {
        match v {
            Value::Str(s) => Ok(s.clone()),
            Value::Nodes(n) => Ok(self.text_of(n)),
            other => err(format!("{what}: expected text, found {}", other.type_name())),
        }
    }

    pub fn num(&self, v: &Value<'a>, what: &str) -> Result<i64, EvalError> {
        match v {
            Value::Num(n) => Ok(*n),
            other => err(format!("{what}: expected a number, found {}", other.type_name())),
        }
    }

    pub fn strings(&self, v: &Value<'a>, what: &str) -> Result<Vec<String>, EvalError> {
        match v {
            Value::List(items) => items.iter().map(|i| self.string(i, what)).collect(),
            Value::Nodes(nodes) => Ok(nodes.iter().map(|n| self.node_text(n).to_string()).collect()),
            Value::Str(s) => Ok(vec![s.clone()]),
            other => err(format!("{what}: expected a list, found {}", other.type_name())),
        }
    }

    pub fn table(&self, name: &str) -> Result<&'a Table, EvalError> {
        self.tables
            .get(name)
            .ok_or_else(|| EvalError(format!("the pack defines no table named '{name}'")))
    }
}

/// Documentation for one function of the fact vocabulary.
#[derive(Clone, Copy, Debug)]
pub struct FactDoc {
    pub name: &'static str,
    pub signature: &'static str,
    pub doc: &'static str,
}

pub fn eval<'a>(expr: &Expr, ctx: &EvalCtx<'a>) -> Result<Value<'a>, EvalError> {
    match expr {
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Capture(name) => Ok(ctx.bindings.get(name).cloned().unwrap_or(Value::Nodes(Vec::new()))),
        Expr::List(items) => Ok(Value::List(
            items.iter().map(|e| eval(e, ctx)).collect::<Result<_, _>>()?,
        )),
        Expr::Not(e) => Ok(Value::Bool(!eval_bool(e, ctx)?)),
        Expr::And(a, b) => Ok(Value::Bool(eval_bool(a, ctx)? && eval_bool(b, ctx)?)),
        Expr::Or(a, b) => Ok(Value::Bool(eval_bool(a, ctx)? || eval_bool(b, ctx)?)),
        Expr::Cmp(a, op, b) => {
            let a = eval(a, ctx)?;
            let b = eval(b, ctx)?;
            compare(&a, *op, &b, ctx).map(Value::Bool)
        }
        Expr::Call(name, args) => {
            let args: Vec<Value<'a>> = args.iter().map(|e| eval(e, ctx)).collect::<Result<_, _>>()?;
            call(name, &args, ctx)
        }
    }
}

/// Facts must be booleans; there is no implicit truthiness.
pub fn eval_bool(expr: &Expr, ctx: &EvalCtx<'_>) -> Result<bool, EvalError> {
    match eval(expr, ctx)? {
        Value::Bool(b) => Ok(b),
        other => err(format!(
            "a fact must be true or false, but this expression is {} (use exists(...) to test nodes)",
            other.type_name()
        )),
    }
}

fn compare<'a>(a: &Value<'a>, op: CmpOp, b: &Value<'a>, ctx: &EvalCtx<'a>) -> Result<bool, EvalError> {
    match op {
        CmpOp::In | CmpOp::NotIn => {
            let needle = ctx.string(a, "left side of 'in'")?;
            let found = match b {
                Value::List(_) | Value::Nodes(_) => ctx.strings(b, "right side of 'in'")?.contains(&needle),
                Value::Str(s) => s.contains(&needle),
                other => {
                    return err(format!(
                        "right side of 'in' must be a list, found {}",
                        other.type_name()
                    ))
                }
            };
            Ok(if op == CmpOp::In { found } else { !found })
        }
        _ => {
            let ord = match (a, b) {
                (Value::Num(x), Value::Num(y)) => x.cmp(y),
                (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
                (Value::Num(_), _) | (_, Value::Num(_)) | (Value::Bool(_), _) | (_, Value::Bool(_)) => {
                    return err(format!("cannot compare {} with {}", a.type_name(), b.type_name()))
                }
                _ => ctx.string(a, "comparison")?.cmp(&ctx.string(b, "comparison")?),
            };
            Ok(match op {
                CmpOp::Eq => ord.is_eq(),
                CmpOp::Ne => !ord.is_eq(),
                CmpOp::Lt => ord.is_lt(),
                CmpOp::Le => ord.is_le(),
                CmpOp::Gt => ord.is_gt(),
                CmpOp::Ge => ord.is_ge(),
                CmpOp::In | CmpOp::NotIn => unreachable!(),
            })
        }
    }
}

fn arity(name: &str, args: &[Value<'_>], min: usize, max: usize) -> Result<(), EvalError> {
    if args.len() < min || args.len() > max {
        let want = if min == max {
            format!("{min}")
        } else if max == usize::MAX {
            format!("at least {min}")
        } else {
            format!("{min} to {max}")
        };
        return err(format!("{name}() takes {want} argument(s), given {}", args.len()));
    }
    Ok(())
}

/// Named, non-extra (non-comment) children.
pub fn children<'a>(node: &Node<'a>) -> Vec<Node<'a>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).filter(|c| !c.is_extra()).collect()
}

fn next_named<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    let mut n = node.next_named_sibling();
    while let Some(s) = n {
        if !s.is_extra() {
            return Some(s);
        }
        n = s.next_named_sibling();
    }
    None
}

fn prev_named<'a>(node: &Node<'a>) -> Option<Node<'a>> {
    let mut n = node.prev_named_sibling();
    while let Some(s) = n {
        if !s.is_extra() {
            return Some(s);
        }
        n = s.prev_named_sibling();
    }
    None
}

pub fn contains_kind(node: &Node<'_>, kinds: &[String]) -> bool {
    let mut cursor = node.walk();
    let mut stack = vec![*node];
    while let Some(n) = stack.pop() {
        if kinds.iter().any(|k| k == n.kind()) {
            return true;
        }
        stack.extend(n.children(&mut cursor));
    }
    false
}

/// Every comment node whose range intersects [start, end).
pub fn comments_in<'a>(file: &'a SourceFile, start: usize, end: usize) -> Vec<Node<'a>> {
    let mut out = Vec::new();
    let mut stack = vec![file.tree.root_node()];
    let mut cursor = file.tree.walk();
    while let Some(n) = stack.pop() {
        if n.end_byte() <= start || n.start_byte() >= end {
            continue;
        }
        if file.lang.is_comment(&n) {
            out.push(n);
            continue;
        }
        stack.extend(n.children(&mut cursor));
    }
    out.sort_by_key(|n| n.start_byte());
    out
}

pub fn line_start(text: &str, byte: usize) -> usize {
    text[..byte].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

pub fn line_end(text: &str, byte: usize) -> usize {
    text[byte..].find('\n').map(|i| byte + i).unwrap_or(text.len())
}

/// True when the nodes span whole lines: nothing but whitespace before the
/// first node on its line, and nothing but whitespace after the last.
pub fn occupies_lines(text: &str, start: usize, end: usize) -> bool {
    let before = &text[line_start(text, start)..start];
    let after = &text[end..line_end(text, end)];
    before.trim().is_empty() && after.trim().is_empty()
}

pub const GENERIC_FACTS: &[FactDoc] = &[
    FactDoc {
        name: "text",
        signature: "text(nodes) -> string",
        doc: "Source text from the start of the first node to the end of the last.",
    },
    FactDoc {
        name: "kind",
        signature: "kind(nodes) -> string",
        doc: "Grammar kind of the first node, e.g. 'identifier'.",
    },
    FactDoc {
        name: "count",
        signature: "count(nodes | list) -> number",
        doc: "Number of nodes in a capture, or items in a list.",
    },
    FactDoc {
        name: "exists",
        signature: "exists(nodes) -> bool",
        doc: "True when the capture holds at least one node.",
    },
    FactDoc {
        name: "empty",
        signature: "empty(nodes) -> bool",
        doc: "True when the capture holds no nodes.",
    },
    FactDoc {
        name: "field",
        signature: "field(nodes, name) -> nodes",
        doc: "Children of the first node stored under a grammar field.",
    },
    FactDoc {
        name: "field_count",
        signature: "field_count(nodes, name) -> number",
        doc: "How many children of the first node are stored under a field.",
    },
    FactDoc {
        name: "children",
        signature: "children(nodes) -> nodes",
        doc: "Named children of the first node, excluding comments.",
    },
    FactDoc {
        name: "parent",
        signature: "parent(nodes) -> nodes",
        doc: "Parent of the first node.",
    },
    FactDoc {
        name: "next",
        signature: "next(nodes) -> nodes",
        doc: "Next named sibling of the last node, skipping comments.",
    },
    FactDoc {
        name: "prev",
        signature: "prev(nodes) -> nodes",
        doc: "Previous named sibling of the first node, skipping comments.",
    },
    FactDoc {
        name: "siblings_after",
        signature: "siblings_after(nodes) -> nodes",
        doc: "All named siblings after the last node, excluding comments.",
    },
    FactDoc {
        name: "ancestor",
        signature: "ancestor(nodes, kind...) -> nodes",
        doc: "Nearest ancestor of the first node with one of the kinds.",
    },
    FactDoc {
        name: "inside",
        signature: "inside(nodes, kind...) -> bool",
        doc: "True when the first node has an ancestor of one of the kinds.",
    },
    FactDoc {
        name: "contains",
        signature: "contains(nodes, kind...) -> bool",
        doc: "True when any node, or any descendant, has one of the kinds.",
    },
    FactDoc {
        name: "has_comment",
        signature: "has_comment(nodes) -> bool",
        doc: "True when a comment lies inside the span of the nodes.",
    },
    FactDoc {
        name: "has_error",
        signature: "has_error(nodes) -> bool",
        doc: "True when the nodes contain a syntax error.",
    },
    FactDoc {
        name: "multiline",
        signature: "multiline(nodes) -> bool",
        doc: "True when the nodes span more than one line.",
    },
    FactDoc {
        name: "same_line",
        signature: "same_line(a, b) -> bool",
        doc: "True when a and b start on the same line.",
    },
    FactDoc {
        name: "occupies_lines",
        signature: "occupies_lines(nodes) -> bool",
        doc: "True when the nodes are alone on their lines (only whitespace around them).",
    },
    FactDoc {
        name: "line",
        signature: "line(nodes) -> number",
        doc: "1-based line where the first node starts.",
    },
    FactDoc {
        name: "matches",
        signature: "matches(text, regex) -> bool",
        doc: "True when the regular expression matches anywhere in the text.",
    },
    FactDoc {
        name: "starts_with",
        signature: "starts_with(text, prefix) -> bool",
        doc: "Text prefix test.",
    },
    FactDoc {
        name: "ends_with",
        signature: "ends_with(text, suffix) -> bool",
        doc: "Text suffix test.",
    },
    FactDoc {
        name: "lower",
        signature: "lower(text) -> string",
        doc: "Lower-cased text.",
    },
    FactDoc {
        name: "len",
        signature: "len(text | list) -> number",
        doc: "Length of text in characters, or of a list.",
    },
    FactDoc {
        name: "in_table",
        signature: "in_table(table, key) -> bool",
        doc: "True when the pack table contains the key.",
    },
    FactDoc {
        name: "table",
        signature: "table(table, key) -> list",
        doc: "The list stored under a key of a pack map table (empty when absent).",
    },
    FactDoc {
        name: "lookup",
        signature: "lookup(table, key) -> string",
        doc: "The first string stored under a key of a pack map table (empty when absent).",
    },
    FactDoc {
        name: "choose",
        signature: "choose(condition, a, b) -> value",
        doc: "a when the condition is true, otherwise b.",
    },
    FactDoc {
        name: "subset",
        signature: "subset(list, list) -> bool",
        doc: "True when every item of the first list is in the second.",
    },
    FactDoc {
        name: "file_name",
        signature: "file_name() -> string",
        doc: "Name of the file being examined, e.g. '__init__.py'.",
    },
    FactDoc {
        name: "file_path",
        signature: "file_path() -> string",
        doc: "Path of the file relative to the project root.",
    },
];

fn call<'a>(name: &str, args: &[Value<'a>], ctx: &EvalCtx<'a>) -> Result<Value<'a>, EvalError> {
    if let Some((prefix, rest)) = name.split_once('.') {
        if prefix != ctx.file.lang.prefix() {
            return err(format!(
                "'{name}' belongs to another language; this rule runs on {} files",
                ctx.file.lang.id()
            ));
        }
        return match ctx.file.lang.call(rest, args, ctx) {
            Some(result) => result,
            None => err(format!("unknown fact '{name}'")),
        };
    }
    let first_nodes = |what: &str| -> Result<Vec<Node<'a>>, EvalError> {
        ctx.nodes(
            args.first()
                .ok_or_else(|| EvalError(format!("{what}() needs an argument")))?,
            what,
        )
    };
    match name {
        "text" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Str(ctx.string(&args[0], name)?))
        }
        "kind" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Str(
                first_nodes(name)?
                    .first()
                    .map(|n| n.kind().to_string())
                    .unwrap_or_default(),
            ))
        }
        "count" | "len" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Num(match &args[0] {
                Value::Nodes(n) if name == "count" => n.len() as i64,
                Value::List(l) => l.len() as i64,
                Value::Str(s) if name == "len" => s.chars().count() as i64,
                Value::Nodes(n) => ctx.text_of(n).chars().count() as i64,
                other => return err(format!("{name}() cannot measure {}", other.type_name())),
            }))
        }
        "exists" | "empty" => {
            arity(name, args, 1, 1)?;
            let any = !first_nodes(name)?.is_empty();
            Ok(Value::Bool(if name == "exists" { any } else { !any }))
        }
        "field" | "field_count" => {
            arity(name, args, 2, 2)?;
            let field = ctx.string(&args[1], name)?;
            let nodes = first_nodes(name)?;
            let found: Vec<Node<'a>> = match nodes.first() {
                Some(n) => {
                    let mut cursor = n.walk();
                    n.children_by_field_name(&field, &mut cursor).collect()
                }
                None => Vec::new(),
            };
            Ok(if name == "field" {
                Value::Nodes(found)
            } else {
                Value::Num(found.len() as i64)
            })
        }
        "children" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Nodes(
                first_nodes(name)?.first().map(children).unwrap_or_default(),
            ))
        }
        "parent" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Nodes(
                first_nodes(name)?
                    .first()
                    .and_then(|n| n.parent())
                    .into_iter()
                    .collect(),
            ))
        }
        "next" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Nodes(
                first_nodes(name)?.last().and_then(next_named).into_iter().collect(),
            ))
        }
        "prev" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Nodes(
                first_nodes(name)?.first().and_then(prev_named).into_iter().collect(),
            ))
        }
        "siblings_after" => {
            arity(name, args, 1, 1)?;
            let mut out = Vec::new();
            let mut cur = first_nodes(name)?.last().and_then(next_named);
            while let Some(n) = cur {
                out.push(n);
                cur = next_named(&n);
            }
            Ok(Value::Nodes(out))
        }
        "ancestor" | "inside" => {
            arity(name, args, 2, usize::MAX)?;
            let kinds: Vec<String> = args[1..]
                .iter()
                .map(|a| ctx.string(a, name))
                .collect::<Result<_, _>>()?;
            let mut found = None;
            let mut cur = first_nodes(name)?.first().and_then(|n| n.parent());
            while let Some(n) = cur {
                if kinds.iter().any(|k| k == n.kind()) {
                    found = Some(n);
                    break;
                }
                cur = n.parent();
            }
            Ok(if name == "ancestor" {
                Value::Nodes(found.into_iter().collect())
            } else {
                Value::Bool(found.is_some())
            })
        }
        "contains" => {
            arity(name, args, 2, usize::MAX)?;
            let kinds: Vec<String> = args[1..]
                .iter()
                .map(|a| ctx.string(a, name))
                .collect::<Result<_, _>>()?;
            Ok(Value::Bool(first_nodes(name)?.iter().any(|n| contains_kind(n, &kinds))))
        }
        "has_comment" => {
            arity(name, args, 1, 1)?;
            let nodes = first_nodes(name)?;
            Ok(Value::Bool(match (nodes.first(), nodes.last()) {
                (Some(a), Some(b)) => !comments_in(ctx.file, a.start_byte(), b.end_byte()).is_empty(),
                _ => false,
            }))
        }
        "has_error" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Bool(first_nodes(name)?.iter().any(|n| n.has_error())))
        }
        "multiline" => {
            arity(name, args, 1, 1)?;
            let nodes = first_nodes(name)?;
            Ok(Value::Bool(match (nodes.first(), nodes.last()) {
                (Some(a), Some(b)) => a.start_position().row != b.end_position().row,
                _ => false,
            }))
        }
        "same_line" => {
            arity(name, args, 2, 2)?;
            let a = ctx.nodes(&args[0], name)?;
            let b = ctx.nodes(&args[1], name)?;
            Ok(Value::Bool(match (a.first(), b.first()) {
                (Some(a), Some(b)) => a.start_position().row == b.start_position().row,
                _ => false,
            }))
        }
        "occupies_lines" => {
            arity(name, args, 1, 1)?;
            let nodes = first_nodes(name)?;
            Ok(Value::Bool(match (nodes.first(), nodes.last()) {
                (Some(a), Some(b)) => occupies_lines(&ctx.file.text, a.start_byte(), b.end_byte()),
                _ => false,
            }))
        }
        "line" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Num(
                first_nodes(name)?
                    .first()
                    .map(|n| n.start_position().row as i64 + 1)
                    .unwrap_or(0),
            ))
        }
        "matches" => {
            arity(name, args, 2, 2)?;
            let text = ctx.string(&args[0], name)?;
            let pattern = ctx.string(&args[1], name)?;
            let re = regex::Regex::new(&pattern).map_err(|e| EvalError(format!("matches(): invalid regex: {e}")))?;
            Ok(Value::Bool(re.is_match(&text)))
        }
        "starts_with" | "ends_with" => {
            arity(name, args, 2, 2)?;
            let text = ctx.string(&args[0], name)?;
            let affix = ctx.string(&args[1], name)?;
            Ok(Value::Bool(if name == "starts_with" {
                text.starts_with(&affix)
            } else {
                text.ends_with(&affix)
            }))
        }
        "lower" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Str(ctx.string(&args[0], name)?.to_lowercase()))
        }
        "in_table" => {
            arity(name, args, 2, 2)?;
            let table = ctx.table(&ctx.string(&args[0], name)?)?;
            Ok(Value::Bool(table.has(&ctx.string(&args[1], name)?)))
        }
        "table" => {
            arity(name, args, 2, 2)?;
            let table_name = ctx.string(&args[0], name)?;
            let key = ctx.string(&args[1], name)?;
            match ctx.table(&table_name)? {
                Table::Map(m) => Ok(Value::List(
                    m.get(&key)
                        .map(|v| v.iter().map(|s| Value::Str(s.clone())).collect())
                        .unwrap_or_default(),
                )),
                Table::Set(_) => err(format!("table '{table_name}' is a set; use in_table()")),
            }
        }
        "lookup" => {
            arity(name, args, 2, 2)?;
            let table_name = ctx.string(&args[0], name)?;
            let key = ctx.string(&args[1], name)?;
            match ctx.table(&table_name)? {
                Table::Map(m) => Ok(Value::Str(
                    m.get(&key).and_then(|v| v.first()).cloned().unwrap_or_default(),
                )),
                Table::Set(_) => err(format!("table '{table_name}' is a set; use in_table()")),
            }
        }
        "choose" => {
            arity(name, args, 3, 3)?;
            match &args[0] {
                Value::Bool(true) => Ok(args[1].clone()),
                Value::Bool(false) => Ok(args[2].clone()),
                other => err(format!(
                    "choose(): the condition must be true or false, found {}",
                    other.type_name()
                )),
            }
        }
        "subset" => {
            arity(name, args, 2, 2)?;
            let a = ctx.strings(&args[0], name)?;
            let b = ctx.strings(&args[1], name)?;
            Ok(Value::Bool(a.iter().all(|x| b.contains(x))))
        }
        "file_name" => {
            arity(name, args, 0, 0)?;
            Ok(Value::Str(ctx.file.file_name()))
        }
        "file_path" => {
            arity(name, args, 0, 0)?;
            Ok(Value::Str(ctx.file.display_path()))
        }
        _ => err(format!("unknown fact '{name}'")),
    }
}

/// True when `name` is a function the generic vocabulary or `lang` provides.
pub fn is_known_fact(name: &str, lang: &dyn crate::lang::Language) -> bool {
    match name.split_once('.') {
        Some((prefix, rest)) => prefix == lang.prefix() && lang.facts().iter().any(|f| f.name == rest),
        None => GENERIC_FACTS.iter().any(|f| f.name == name),
    }
}
