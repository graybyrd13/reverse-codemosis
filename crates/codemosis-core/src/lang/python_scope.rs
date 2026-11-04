//! Name-binding analysis for Python.
//!
//! Records, for every identifier, whether it binds or reads a name and in
//! which scope, following Python's scoping rules: functions, lambdas, classes
//! and comprehensions open scopes; parameter defaults and annotations are
//! evaluated in the enclosing scope; `:=` binds in the nearest enclosing
//! non-comprehension scope.

use crate::project::SourceFile;
use std::collections::BTreeSet;
use tree_sitter::Node;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Function,
    Class,
    Comprehension,
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub kind: ScopeKind,
    pub start: usize,
    pub end: usize,
    pub parent: Option<usize>,
    pub globals: BTreeSet<String>,
    pub nonlocals: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Binding,
    Reference,
}

#[derive(Clone, Debug)]
pub struct Occurrence {
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub role: Role,
    pub scope: usize,
    /// For a reference used as `name.attr`, the attribute.
    pub attr: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct PyAnalysis {
    pub scopes: Vec<Scope>,
    pub occurrences: Vec<Occurrence>,
    /// The file calls globals(), locals(), vars(), exec(), eval() or
    /// __import__(), or defines a module-level __getattr__.
    pub dynamic: bool,
    pub star_import: bool,
    /// Every identifier-like word that appears in a string or a comment.
    pub text_words: BTreeSet<String>,
}

impl PyAnalysis {
    pub fn bindings_of<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s Occurrence> + 's {
        self.occurrences
            .iter()
            .filter(move |o| o.role == Role::Binding && o.name == name)
    }

    pub fn references_of<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s Occurrence> + 's {
        self.occurrences
            .iter()
            .filter(move |o| o.role == Role::Reference && o.name == name)
    }

    /// True when anything in the file binds or declares `name`.
    pub fn is_bound(&self, name: &str) -> bool {
        self.bindings_of(name).next().is_some()
            || self
                .scopes
                .iter()
                .any(|s| s.globals.contains(name) || s.nonlocals.contains(name))
    }

    /// The innermost scope containing the byte range.
    pub fn scope_at(&self, start: usize, end: usize) -> usize {
        let mut best = 0;
        for (i, s) in self.scopes.iter().enumerate() {
            if s.start <= start && end <= s.end {
                let b = &self.scopes[best];
                if s.end - s.start <= b.end - b.start {
                    best = i;
                }
            }
        }
        best
    }

    /// True when `name` is bound in `scope` or any scope enclosing it.
    pub fn visible_from(&self, scope: usize, name: &str) -> bool {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if self.bindings_of(name).any(|o| o.scope == s) {
                return true;
            }
            cur = self.scopes[s].parent;
        }
        false
    }
}

pub fn analyze(file: &SourceFile) -> PyAnalysis {
    let root = file.tree.root_node();
    let mut w = Walker {
        text: &file.text,
        out: PyAnalysis::default(),
    };
    w.out.scopes.push(Scope {
        kind: ScopeKind::Module,
        start: 0,
        end: file.text.len(),
        parent: None,
        globals: BTreeSet::new(),
        nonlocals: BTreeSet::new(),
    });
    w.visit(root, 0);
    w.out
}

const DYNAMIC_CALLS: &[&str] = &["globals", "locals", "vars", "exec", "eval", "__import__"];

struct Walker<'a> {
    text: &'a str,
    out: PyAnalysis,
}

fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn field_children<'t>(node: Node<'t>, field: &str) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor).collect()
}

impl<'a> Walker<'a> {
    fn text(&self, n: Node) -> &'a str {
        &self.text[n.byte_range()]
    }

    fn new_scope(&mut self, kind: ScopeKind, node: Node, parent: usize) -> usize {
        self.out.scopes.push(Scope {
            kind,
            start: node.start_byte(),
            end: node.end_byte(),
            parent: Some(parent),
            globals: BTreeSet::new(),
            nonlocals: BTreeSet::new(),
        });
        self.out.scopes.len() - 1
    }

    fn effective(&self, mut scope: usize) -> usize {
        while self.out.scopes[scope].kind == ScopeKind::Comprehension {
            scope = self.out.scopes[scope].parent.unwrap_or(0);
        }
        scope
    }

    fn record(&mut self, node: Node, role: Role, scope: usize, attr: Option<String>) {
        self.out.occurrences.push(Occurrence {
            start: node.start_byte(),
            end: node.end_byte(),
            name: self.text(node).to_string(),
            role,
            scope,
            attr,
        });
    }

    fn bind(&mut self, node: Node, scope: usize) {
        if node.kind() == "identifier" {
            self.record(node, Role::Binding, scope, None);
        } else {
            self.target(node, scope);
        }
    }

    fn reference(&mut self, node: Node, scope: usize) {
        let attr = node.parent().and_then(|p| {
            (p.kind() == "attribute" && p.child_by_field_name("object").map(|o| o.id()) == Some(node.id()))
                .then(|| p.child_by_field_name("attribute").map(|a| self.text(a).to_string()))
                .flatten()
        });
        self.record(node, Role::Reference, scope, attr);
    }

    fn words(&mut self, node: Node) {
        let text = self.text(node);
        let mut word = String::new();
        for c in text.chars().chain(std::iter::once(' ')) {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
            } else if !word.is_empty() {
                if !word.starts_with(|ch: char| ch.is_ascii_digit()) {
                    self.out.text_words.insert(std::mem::take(&mut word));
                } else {
                    word.clear();
                }
            }
        }
    }

    fn visit_children(&mut self, node: Node, scope: usize) {
        for c in named_children(node) {
            self.visit(c, scope);
        }
    }

    fn visit(&mut self, node: Node, scope: usize) {
        match node.kind() {
            "identifier" => self.reference(node, scope),
            "comment" | "string_content" => self.words(node),
            "function_definition" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.bind(name, scope);
                    if scope == 0 && self.text(name) == "__getattr__" {
                        self.out.dynamic = true;
                    }
                }
                let inner = self.new_scope(ScopeKind::Function, node, scope);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.parameters(params, scope, inner);
                }
                for f in ["return_type", "type_parameters"] {
                    if let Some(n) = node.child_by_field_name(f) {
                        self.visit(n, scope);
                    }
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, inner);
                }
            }
            "lambda" => {
                let inner = self.new_scope(ScopeKind::Function, node, scope);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.parameters(params, scope, inner);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, inner);
                }
            }
            "class_definition" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.bind(name, scope);
                }
                for f in ["superclasses", "type_parameters"] {
                    if let Some(n) = node.child_by_field_name(f) {
                        self.visit(n, scope);
                    }
                }
                let inner = self.new_scope(ScopeKind::Class, node, scope);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit(body, inner);
                }
            }
            "list_comprehension" | "set_comprehension" | "dictionary_comprehension" | "generator_expression" => {
                let inner = self.new_scope(ScopeKind::Comprehension, node, scope);
                let mut first_clause = true;
                for c in named_children(node) {
                    if c.kind() == "for_in_clause" {
                        for left in field_children(c, "left") {
                            self.target(left, inner);
                        }
                        let eval_scope = if first_clause { scope } else { inner };
                        for right in field_children(c, "right") {
                            self.visit(right, eval_scope);
                        }
                        first_clause = false;
                    } else {
                        self.visit(c, inner);
                    }
                }
            }
            "assignment" => {
                for left in field_children(node, "left") {
                    self.target(left, scope);
                }
                for f in ["right", "type"] {
                    for n in field_children(node, f) {
                        self.visit(n, scope);
                    }
                }
            }
            "augmented_assignment" => {
                if let Some(left) = node.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        self.reference(left, scope);
                    }
                    self.target(left, scope);
                }
                if let Some(right) = node.child_by_field_name("right") {
                    self.visit(right, scope);
                }
            }
            "for_statement" => {
                for c in named_children(node) {
                    if node.child_by_field_name("left").map(|l| l.id()) == Some(c.id()) {
                        self.target(c, scope);
                    } else {
                        self.visit(c, scope);
                    }
                }
            }
            "as_pattern" => {
                let mut after_as = false;
                let mut cursor = node.walk();
                let kids: Vec<(Node, Option<&str>)> = node
                    .children(&mut cursor)
                    .enumerate()
                    .map(|(i, c)| (c, node.field_name_for_child(i as u32)))
                    .collect();
                for (c, field) in kids {
                    if !c.is_named() {
                        after_as |= c.kind() == "as";
                        continue;
                    }
                    if field == Some("alias") || (after_as && c.kind() == "identifier") {
                        self.target(c, scope);
                    } else if c.kind() == "case_pattern" {
                        self.case_pattern(c, scope);
                    } else {
                        self.visit(c, scope);
                    }
                }
            }
            "except_clause" => {
                let values = field_children(node, "value");
                for c in named_children(node) {
                    // Legacy `except E, name:` binds its second value.
                    if values.len() == 2 && c.id() == values[1].id() {
                        self.target(c, scope);
                    } else {
                        self.visit(c, scope);
                    }
                }
            }
            "import_statement" => {
                for name in field_children(node, "name") {
                    self.import_name(name, scope, true);
                }
            }
            "import_from_statement" => {
                for name in field_children(node, "name") {
                    self.import_name(name, scope, false);
                }
                if named_children(node).iter().any(|c| c.kind() == "wildcard_import") {
                    self.out.star_import = true;
                }
            }
            "future_import_statement" => {}
            "global_statement" | "nonlocal_statement" => {
                let names: Vec<String> = named_children(node)
                    .into_iter()
                    .filter(|c| c.kind() == "identifier")
                    .map(|c| self.text(c).to_string())
                    .collect();
                let s = &mut self.out.scopes[scope];
                let set = if node.kind() == "global_statement" {
                    &mut s.globals
                } else {
                    &mut s.nonlocals
                };
                set.extend(names);
            }
            "named_expression" => {
                let target_scope = self.effective(scope);
                if let Some(name) = node.child_by_field_name("name") {
                    self.bind(name, target_scope);
                }
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value, scope);
                }
            }
            "delete_statement" => {
                for c in named_children(node) {
                    self.target(c, scope);
                }
            }
            "keyword_argument" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit(value, scope);
                }
            }
            "attribute" => {
                if let Some(obj) = node.child_by_field_name("object") {
                    self.visit(obj, scope);
                }
            }
            "call" => {
                if let Some(f) = node.child_by_field_name("function") {
                    if f.kind() == "identifier" && DYNAMIC_CALLS.contains(&self.text(f)) {
                        self.out.dynamic = true;
                    }
                }
                self.visit_children(node, scope);
            }
            "case_pattern" => self.case_pattern(node, scope),
            _ => self.visit_children(node, scope),
        }
    }

    fn import_name(&mut self, name: Node, scope: usize, plain_import: bool) {
        match name.kind() {
            "dotted_name" => {
                // `import a.b` binds `a`; `from m import a` binds `a`.
                if let Some(first) = named_children(name).into_iter().next() {
                    if plain_import || name.named_child_count() == 1 {
                        self.record(first, Role::Binding, scope, None);
                    }
                }
            }
            "aliased_import" => {
                if let Some(alias) = name.child_by_field_name("alias") {
                    self.record(alias, Role::Binding, scope, None);
                }
            }
            _ => {}
        }
    }

    fn parameters(&mut self, params: Node, outer: usize, inner: usize) {
        for p in named_children(params) {
            match p.kind() {
                "identifier" => self.bind(p, inner),
                "default_parameter" | "typed_default_parameter" => {
                    if let Some(n) = p.child_by_field_name("name") {
                        self.bind(n, inner);
                    }
                    for f in ["type", "value"] {
                        if let Some(n) = p.child_by_field_name(f) {
                            self.visit(n, outer);
                        }
                    }
                }
                "typed_parameter" => {
                    for c in named_children(p) {
                        if p.child_by_field_name("type").map(|t| t.id()) == Some(c.id()) {
                            self.visit(c, outer);
                        } else {
                            self.target(c, inner);
                        }
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" | "tuple_pattern" => self.target(p, inner),
                "comment" => self.words(p),
                _ => {}
            }
        }
    }

    fn target(&mut self, node: Node, scope: usize) {
        match node.kind() {
            "identifier" => self.record(node, Role::Binding, scope, None),
            "pattern_list"
            | "tuple_pattern"
            | "list_pattern"
            | "tuple"
            | "list"
            | "expression_list"
            | "parenthesized_expression"
            | "list_splat_pattern"
            | "list_splat"
            | "dictionary_splat_pattern"
            | "as_pattern_target" => {
                for c in named_children(node) {
                    self.target(c, scope);
                }
            }
            "comment" => self.words(node),
            _ => self.visit(node, scope),
        }
    }

    fn case_pattern(&mut self, node: Node, scope: usize) {
        match node.kind() {
            "dotted_name" => {
                let parts = named_children(node);
                if parts.len() == 1 {
                    self.record(parts[0], Role::Binding, scope, None);
                } else if let Some(first) = parts.first() {
                    self.reference(*first, scope);
                }
            }
            "class_pattern" => {
                let mut first = true;
                for c in named_children(node) {
                    if first && c.kind() == "dotted_name" {
                        if let Some(id) = named_children(c).into_iter().next() {
                            self.reference(id, scope);
                        }
                    } else {
                        self.case_pattern(c, scope);
                    }
                    first = false;
                }
            }
            "keyword_pattern" => {
                for (i, c) in named_children(node).into_iter().enumerate() {
                    if i > 0 {
                        self.case_pattern(c, scope);
                    }
                }
            }
            "splat_pattern" => {
                for c in named_children(node) {
                    if c.kind() == "identifier" {
                        self.record(c, Role::Binding, scope, None);
                    }
                }
            }
            "as_pattern" => self.visit(node, scope),
            "case_pattern" | "list_pattern" | "tuple_pattern" | "union_pattern" => {
                for c in named_children(node) {
                    self.case_pattern(c, scope);
                }
            }
            "dict_pattern" => {
                for c in named_children(node) {
                    if c.kind() == "case_pattern" {
                        self.case_pattern(c, scope);
                    } else {
                        self.visit(c, scope);
                    }
                }
            }
            _ => self.visit(node, scope),
        }
    }
}
