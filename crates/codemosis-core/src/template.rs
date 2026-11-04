//! Rewrite templates.
//!
//! A template is literal text with holes: `${capture}` inserts the source
//! text of a capture, `${capture|filter|filter:arg}` passes it through
//! filters first, `${indent}` inserts one indentation unit of the file, and
//! `$$` is a literal dollar sign.

use crate::eval::{err, EvalCtx, EvalError, Value};
use crate::lang::Language;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
enum Segment {
    Text(String),
    Hole {
        name: String,
        filters: Vec<(String, Option<String>)>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    segments: Vec<Segment>,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateError(pub String);

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TemplateError {}

/// Filters every language can use.
pub const GENERIC_FILTERS: &[(&str, &str)] = &[
    ("trim", "Remove surrounding whitespace."),
    ("lower", "Lower-case the text."),
    ("upper", "Upper-case the text."),
    ("line", "The 1-based line number of the capture instead of its text."),
    (
        "strip_parens",
        "Remove one pair of parentheses that encloses the whole text.",
    ),
    ("unescape_braces", "Turn '{{' into '{' and '}}' into '}'."),
    (
        "drop_string_prefix",
        "Remove a letter (the argument) from a string literal's prefix: ${s|drop_string_prefix:f}.",
    ),
    (
        "add_string_prefix",
        "Add a letter (the argument) to a string literal's prefix: ${s|add_string_prefix:f}.",
    ),
];

impl Template {
    pub fn parse(src: &str) -> Result<Template, TemplateError> {
        let mut segments = Vec::new();
        let mut text = String::new();
        let mut rest = src;
        while let Some(i) = rest.find('$') {
            text.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            if let Some(stripped) = after.strip_prefix('$') {
                text.push('$');
                rest = stripped;
            } else if let Some(body_start) = after.strip_prefix('{') {
                let close = body_start
                    .find('}')
                    .ok_or_else(|| TemplateError(format!("unclosed '${{' in template: {src}")))?;
                let body = &body_start[..close];
                let mut parts = body.split('|');
                let name = parts.next().unwrap_or("").trim().to_string();
                if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return Err(TemplateError(format!("'${{{body}}}' must start with a capture name")));
                }
                let filters = parts
                    .map(|f| {
                        let f = f.trim();
                        match f.split_once(':') {
                            Some((n, a)) => (n.trim().to_string(), Some(a.trim().to_string())),
                            None => (f.to_string(), None),
                        }
                    })
                    .collect();
                if !text.is_empty() {
                    segments.push(Segment::Text(std::mem::take(&mut text)));
                }
                segments.push(Segment::Hole { name, filters });
                rest = &body_start[close + 1..];
            } else {
                text.push('$');
                rest = after;
            }
        }
        text.push_str(rest);
        if !text.is_empty() {
            segments.push(Segment::Text(text));
        }
        Ok(Template {
            segments,
            source: src.to_string(),
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// Capture names the template reads (excluding `indent`).
    pub fn names(&self) -> Vec<String> {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Hole { name, .. } if name != "indent" => Some(name.clone()),
                _ => None,
            })
            .collect()
    }

    /// Filter names the template uses.
    pub fn filters(&self) -> Vec<String> {
        self.segments
            .iter()
            .flat_map(|s| match s {
                Segment::Hole { filters, .. } => filters.iter().map(|f| f.0.clone()).collect(),
                _ => Vec::new(),
            })
            .collect()
    }

    pub fn render(&self, ctx: &EvalCtx<'_>) -> Result<String, EvalError> {
        let mut out = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Text(t) => out.push_str(t),
                Segment::Hole { name, filters } => {
                    if name == "indent" && !ctx.bindings.contains_key("indent") {
                        out.push_str(&ctx.file.lang.indent_unit(&ctx.file.text));
                        continue;
                    }
                    let Some(value) = ctx.bindings.get(name) else {
                        return err(format!("template refers to unknown capture '{name}'"));
                    };
                    let mut value = value.clone();
                    for (filter, arg) in filters {
                        value = apply_filter(filter, arg.as_deref(), value, ctx)?;
                    }
                    out.push_str(&ctx.string(&value, "template")?);
                }
            }
        }
        Ok(out)
    }
}

pub fn is_known_filter(name: &str, lang: &dyn Language) -> bool {
    match name.split_once('.') {
        Some((prefix, rest)) => prefix == lang.prefix() && lang.filters().iter().any(|(n, _)| *n == rest),
        None => GENERIC_FILTERS.iter().any(|(n, _)| *n == name),
    }
}

fn apply_filter<'a>(
    name: &str,
    arg: Option<&str>,
    value: Value<'a>,
    ctx: &EvalCtx<'a>,
) -> Result<Value<'a>, EvalError> {
    if let Some((prefix, rest)) = name.split_once('.') {
        if prefix == ctx.file.lang.prefix() {
            if let Some(r) = ctx.file.lang.filter(rest, arg, &value, ctx) {
                return r;
            }
        }
        return err(format!("unknown filter '{name}'"));
    }
    if name == "line" {
        let nodes = ctx.nodes(&value, "line")?;
        return Ok(Value::Str(
            nodes
                .first()
                .map(|n| (n.start_position().row + 1).to_string())
                .unwrap_or_default(),
        ));
    }
    let text = ctx.string(&value, name)?;
    let need_arg = || {
        arg.filter(|a| !a.is_empty())
            .ok_or_else(|| EvalError(format!("filter '{name}' needs an argument, like {name}:f")))
    };
    let out = match name {
        "trim" => text.trim().to_string(),
        "lower" => text.to_lowercase(),
        "upper" => text.to_uppercase(),
        "strip_parens" => strip_parens(&text),
        "unescape_braces" => text.replace("{{", "{").replace("}}", "}"),
        "drop_string_prefix" | "add_string_prefix" => {
            let letter = need_arg()?;
            let quote = text
                .find(['"', '\''])
                .ok_or_else(|| EvalError(format!("{name}: '{text}' is not a string literal")))?;
            let (prefix, body) = text.split_at(quote);
            if name == "drop_string_prefix" {
                let kept: String = prefix
                    .chars()
                    .filter(|c| !letter.eq_ignore_ascii_case(&c.to_string()))
                    .collect();
                format!("{kept}{body}")
            } else if prefix.to_lowercase().contains(&letter.to_lowercase()) {
                text.clone()
            } else {
                format!("{letter}{prefix}{body}")
            }
        }
        other => return err(format!("unknown filter '{other}'")),
    };
    Ok(Value::Str(out))
}

/// Remove one pair of parentheses that encloses the whole text.
pub fn strip_parens(text: &str) -> String {
    let t = text.trim();
    if !(t.starts_with('(') && t.ends_with(')')) {
        return text.to_string();
    }
    let mut depth = 0i32;
    for (i, c) in t.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i != t.len() - 1 {
                    return text.to_string();
                }
            }
            _ => {}
        }
    }
    t[1..t.len() - 1].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_holes_and_filters() {
        let t = Template::parse("if ${name} is None:\n${indent}${name} = ${value|trim}$$").unwrap();
        assert_eq!(t.names(), vec!["name", "name", "value"]);
        assert_eq!(t.filters(), vec!["trim"]);
        assert!(Template::parse("${").is_err());
        assert!(Template::parse("${a b}").is_err());
    }

    #[test]
    fn strips_only_enclosing_parens() {
        assert_eq!(strip_parens("(a, b)"), "a, b");
        assert_eq!(strip_parens("(a) + (b)"), "(a) + (b)");
        assert_eq!(strip_parens("x"), "x");
    }
}
