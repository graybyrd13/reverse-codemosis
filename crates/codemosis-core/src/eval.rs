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
    /// Zero or more byte ranges of the file, in order (captures of text rules).
    Spans(Vec<(usize, usize)>),
    List(Vec<Value<'a>>),
}

impl Value<'_> {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "bool",
            Value::Num(_) => "number",
            Value::Str(_) => "string",
            Value::Nodes(_) => "nodes",
            Value::Spans(_) => "spans",
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

    /// The byte ranges of a capture, whether it holds nodes or spans.
    pub fn ranges(&self, v: &Value<'a>, what: &str) -> Result<Vec<(usize, usize)>, EvalError> {
        match v {
            Value::Nodes(n) => Ok(n.iter().map(|n| (n.start_byte(), n.end_byte())).collect()),
            Value::Spans(s) => Ok(s.clone()),
            other => err(format!("{what}: expected a capture, found {}", other.type_name())),
        }
    }

    /// The overall range of a capture: from its first start to its last end.
    pub fn extent(&self, v: &Value<'a>, what: &str) -> Result<Option<(usize, usize)>, EvalError> {
        let r = self.ranges(v, what)?;
        Ok(match (r.first(), r.last()) {
            (Some(a), Some(b)) => Some((a.0, b.1.max(a.0))),
            _ => None,
        })
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
            Value::Spans(_) => err(format!(
                "{what}: needs syntax-tree nodes, but this rule matches text (it has a `pattern`, not a `shape`)"
            )),
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
            Value::Spans(s) => Ok(match (s.first(), s.last()) {
                (Some(a), Some(b)) => self.file.text[a.0..b.1.max(a.0)].to_string(),
                _ => String::new(),
            }),
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
            Value::Spans(spans) => Ok(spans.iter().map(|(a, b)| self.file.text[*a..*b].to_string()).collect()),
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
                Value::List(_) | Value::Nodes(_) | Value::Spans(_) => {
                    ctx.strings(b, "right side of 'in'")?.contains(&needle)
                }
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

/// Every comment whose range intersects [start, end), from the syntax tree
/// or the lexical profile. Empty when the file's comments cannot be known.
pub fn comments_in(file: &SourceFile, start: usize, end: usize) -> Vec<(usize, usize)> {
    file.segments()
        .iter()
        .filter(|s| s.kind == crate::lex::SegKind::Comment && s.end > start && s.start < end)
        .map(|s| (s.start, s.end))
        .collect()
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
        name: "earlier_same_field",
        signature: "earlier_same_field(nodes, field) -> nodes",
        doc: "The `field` child of the first earlier named sibling whose `field` child has exactly the same text as the node's own (for duplicate keys: `earlier_same_field(@pair, 'key')`). Empty when there is none. Linear in the number of siblings, unlike a shape that pairs every sibling with every other.",
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
        name: "concat",
        signature: "concat(text...) -> string",
        doc: "The texts joined together.",
    },
    FactDoc {
        name: "bang",
        signature: "bang(text) -> string",
        doc: "The C-style negation of an expression: `!x`, parenthesised unless x is a single name, member access or call.",
    },
    FactDoc {
        name: "count_matches",
        signature: "count_matches(text, regex) -> number",
        doc: "How many times the regular expression matches in the text.",
    },
    FactDoc {
        name: "file_matches",
        signature: "file_matches(regex) -> number",
        doc: "How many times the regular expression matches in the whole file, in code only (not comments or strings) when the file's comments are known.",
    },
    FactDoc {
        name: "kind_at",
        signature: "kind_at(capture) -> string",
        doc: "What the capture starts in: 'code', 'comment', 'string', or 'unknown' when the file's comments and strings cannot be told apart.",
    },
    FactDoc {
        name: "language",
        signature: "language() -> string",
        doc: "The file's language, as GitHub names it: 'Python', 'Shell', 'PowerShell'...",
    },
    FactDoc {
        name: "language_kind",
        signature: "language_kind() -> string",
        doc: "GitHub's type for the file's language: 'programming', 'markup', 'data' or 'prose'.",
    },
    FactDoc {
        name: "tier",
        signature: "tier() -> string",
        doc: "How deeply the file is understood: 'deep' (syntax tree), 'lexical' (comments and strings) or 'recognized'.",
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
        return match ctx.file.lang {
            Some(lang) if lang.prefix() == prefix => match lang.call(rest, args, ctx) {
                Some(result) => result,
                None => err(format!("unknown fact '{name}'")),
            },
            _ => err(format!(
                "'{name}' belongs to another language; this file is {}",
                ctx.file.language_name()
            )),
        };
    }
    let extent = |what: &str| -> Result<Option<(usize, usize)>, EvalError> {
        ctx.extent(
            args.first()
                .ok_or_else(|| EvalError(format!("{what}() needs an argument")))?,
            what,
        )
    };
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
                Value::Spans(n) if name == "count" => n.len() as i64,
                Value::Spans(_) => ctx.string(&args[0], name)?.chars().count() as i64,
                Value::List(l) => l.len() as i64,
                Value::Str(s) if name == "len" => s.chars().count() as i64,
                Value::Nodes(n) => ctx.text_of(n).chars().count() as i64,
                other => return err(format!("{name}() cannot measure {}", other.type_name())),
            }))
        }
        "exists" | "empty" => {
            arity(name, args, 1, 1)?;
            let any = !ctx.ranges(&args[0], name)?.is_empty();
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
        "earlier_same_field" => {
            arity(name, args, 2, 2)?;
            let field = ctx.string(&args[1], name)?;
            let Some(node) = first_nodes(name)?.first().copied() else {
                return Ok(Value::Nodes(Vec::new()));
            };
            let Some(own) = node.child_by_field_name(&field) else {
                return Ok(Value::Nodes(Vec::new()));
            };
            let own_text = &ctx.file.text[own.byte_range()];
            // Walk forward from the first sibling: tree-sitter finds a
            // previous sibling by rescanning from the parent's start, so a
            // backward walk would be quadratic per call.
            let Some(parent) = node.parent() else {
                return Ok(Value::Nodes(Vec::new()));
            };
            // Where each text first appears among the siblings, computed once
            // per parent, so the whole container costs linear time.
            let firsts = ctx
                .file
                .memo(format!("earlier_same_field:{}:{field}", parent.id()), || {
                    let mut firsts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
                    let mut walker = parent.walk();
                    for sib in parent.named_children(&mut walker) {
                        if let Some(k) = sib.child_by_field_name(&field) {
                            firsts
                                .entry(ctx.file.text[k.byte_range()].to_string())
                                .or_insert(k.start_byte());
                        }
                    }
                    firsts
                });
            let Some(&first_start) = firsts.get(own_text) else {
                return Ok(Value::Nodes(Vec::new()));
            };
            if first_start >= own.start_byte() {
                return Ok(Value::Nodes(Vec::new()));
            }
            // A real duplicate (rare): find the earlier node itself.
            let mut walker = parent.walk();
            let found = parent
                .named_children(&mut walker)
                .filter_map(|sib| sib.child_by_field_name(&field))
                .find(|k| k.start_byte() == first_start);
            Ok(Value::Nodes(found.into_iter().collect()))
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
            Ok(Value::Bool(match extent(name)? {
                Some((a, b)) => !comments_in(ctx.file, a, b).is_empty(),
                None => false,
            }))
        }
        "has_error" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Bool(first_nodes(name)?.iter().any(|n| n.has_error())))
        }
        "multiline" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Bool(match extent(name)? {
                Some((a, b)) => ctx.file.text[a..b].contains('\n'),
                None => false,
            }))
        }
        "same_line" => {
            arity(name, args, 2, 2)?;
            let a = ctx.extent(&args[0], name)?;
            let b = ctx.extent(&args[1], name)?;
            Ok(Value::Bool(match (a, b) {
                (Some(a), Some(b)) => ctx.file.line_of(a.0) == ctx.file.line_of(b.0),
                _ => false,
            }))
        }
        "occupies_lines" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Bool(match extent(name)? {
                Some((a, b)) => occupies_lines(&ctx.file.text, a, b),
                None => false,
            }))
        }
        "line" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Num(
                extent(name)?.map(|(a, _)| ctx.file.line_of(a) as i64).unwrap_or(0),
            ))
        }
        "matches" => {
            arity(name, args, 2, 2)?;
            let text = ctx.string(&args[0], name)?;
            let pattern = ctx.string(&args[1], name)?;
            let re =
                crate::regex_cache::get(&pattern).map_err(|e| EvalError(format!("matches(): invalid regex: {e}")))?;
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
        "concat" => {
            let parts: Vec<String> = args.iter().map(|a| ctx.string(a, name)).collect::<Result<_, _>>()?;
            Ok(Value::Str(parts.concat()))
        }
        "bang" => {
            arity(name, args, 1, 1)?;
            Ok(Value::Str(crate::template::bang(&ctx.string(&args[0], name)?)))
        }
        "count_matches" => {
            arity(name, args, 2, 2)?;
            let text = ctx.string(&args[0], name)?;
            let re = crate::regex_cache::get(&ctx.string(&args[1], name)?)
                .map_err(|e| EvalError(format!("count_matches(): invalid regex: {e}")))?;
            Ok(Value::Num(re.find_iter(&text).count() as i64))
        }
        "file_matches" => {
            arity(name, args, 1, 1)?;
            let re = crate::regex_cache::get(&ctx.string(&args[0], name)?)
                .map_err(|e| EvalError(format!("file_matches(): invalid regex: {e}")))?;
            let file = ctx.file;
            let n = re
                .find_iter(&file.text)
                .filter(|m| {
                    !matches!(
                        file.kind_at(m.start()),
                        Some(crate::lex::SegKind::Comment | crate::lex::SegKind::String)
                    )
                })
                .count();
            Ok(Value::Num(n as i64))
        }
        "kind_at" => {
            arity(name, args, 1, 1)?;
            let kind = match extent(name)? {
                Some((a, _)) => ctx.file.kind_at(a).map(|k| k.as_str()).unwrap_or("unknown"),
                None => "unknown",
            };
            Ok(Value::Str(kind.to_string()))
        }
        "language" => {
            arity(name, args, 0, 0)?;
            Ok(Value::Str(ctx.file.language_name().to_string()))
        }
        "language_kind" => {
            arity(name, args, 0, 0)?;
            Ok(Value::Str(
                ctx.file.linguist().map(|l| l.kind.as_str()).unwrap_or("").to_string(),
            ))
        }
        "tier" => {
            arity(name, args, 0, 0)?;
            Ok(Value::Str(ctx.file.tier().as_str().to_string()))
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

/// True when `name` is a function the generic vocabulary or one of `langs`
/// provides.
pub fn is_known_fact(name: &str, langs: &[&'static dyn crate::lang::Language]) -> bool {
    match name.split_once('.') {
        Some((prefix, rest)) => {
            !langs.is_empty()
                && langs
                    .iter()
                    .all(|l| l.prefix() == prefix && l.facts().iter().any(|f| f.name == rest))
        }
        None => GENERIC_FACTS.iter().any(|f| f.name == name),
    }
}
