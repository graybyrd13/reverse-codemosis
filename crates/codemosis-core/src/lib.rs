//! # codemosis-core
//!
//! The foundation of Reverse Codemosis: the model of findings and
//! corrections, the fix-pack format, the fact language that states when a
//! correction is safe, and the matcher that finds where rules apply.
//!
//! The engine provides the vocabulary (facts, filters, edit operations);
//! packs provide the rules. Nothing here guesses: a correction is offered
//! only when every fact its rule requires holds, and every fact answers
//! conservatively.

pub mod edit;
pub mod eval;
pub mod expr;
pub mod lang;
pub mod lex;
pub mod linguist;
pub mod model;
pub mod pack;
pub mod process;
pub mod project;
pub mod rule;
pub mod selftest;
pub mod tables;
pub mod template;

pub use model::{
    Category, Distance, Edit, Evidence, Finding, Position, Question, QuestionOption, Semantics, Span, Status,
};
pub use pack::Pack;
pub use project::{FileFilter, Project, SourceFile, Tier};
pub use rule::{scan, CompiledRule, Scan};

/// Render a syntax tree with field names and anonymous tokens, to help pack
/// authors write shapes.
pub fn render_tree(file: &SourceFile) -> String {
    fn walk(c: &mut tree_sitter::TreeCursor, src: &str, depth: usize, out: &mut String) {
        loop {
            let n = c.node();
            let field = c.field_name().map(|f| format!("{f}: ")).unwrap_or_default();
            let kind = if n.is_named() {
                n.kind().to_string()
            } else {
                format!("\"{}\"", n.kind())
            };
            let flags = if n.is_error() {
                " ERROR"
            } else if n.is_missing() {
                " MISSING"
            } else {
                ""
            };
            let leaf = if n.child_count() == 0 && n.is_named() {
                format!(" {:?}", &src[n.byte_range()])
            } else {
                String::new()
            };
            let pos = n.start_position();
            out.push_str(&format!(
                "{}{}{}{}{}  [{}:{}]\n",
                "  ".repeat(depth),
                field,
                kind,
                flags,
                leaf,
                pos.row + 1,
                pos.column + 1
            ));
            if c.goto_first_child() {
                walk(c, src, depth + 1, out);
                c.goto_parent();
            }
            if !c.goto_next_sibling() {
                break;
            }
        }
    }
    let mut out = String::new();
    if let Some(tree) = &file.tree {
        walk(&mut tree.walk(), &file.text, 0, &mut out);
    }
    out
}
