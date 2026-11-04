//! The vocabulary every other part of Reverse Codemosis speaks: categories of
//! shortfall, findings, edits, questions and the distance measure.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// The kinds of distance between code and the best version of itself.
///
/// The order is the order of priority: the engine always closes the most
/// serious kind of distance first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    /// Errors that break the build or make code fail every time it runs.
    Broken,
    /// Logic that contradicts itself.
    Contradiction,
    /// Unsafe patterns with known safe forms.
    Hazard,
    /// Dead paths and wasted work.
    Waste,
    /// Structures that a simpler structure provably replaces.
    Simplification,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Broken,
        Category::Contradiction,
        Category::Hazard,
        Category::Waste,
        Category::Simplification,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Broken => "broken",
            Category::Contradiction => "contradiction",
            Category::Hazard => "hazard",
            Category::Waste => "waste",
            Category::Simplification => "simplification",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Category::Broken => "errors that break the build or the run",
            Category::Contradiction => "logic that contradicts itself",
            Category::Hazard => "unsafe patterns with known safe forms",
            Category::Waste => "dead paths and wasted work",
            Category::Simplification => "structures a simpler structure provably replaces",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn parse(s: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

impl fmt::Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a correction does to the behaviour of the program. Every rule that
/// carries a fix must declare one, and the pack self-test checks the claim by
/// running the rule's examples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Semantics {
    /// Observable behaviour is identical before and after.
    Preserved,
    /// Behaviour changes from something self-contradictory to the only
    /// meaning the code can have had.
    Corrected,
    /// Code that cannot run as written (does not compile, or fails every time
    /// the line is reached) now runs, through the only possible repair.
    Repaired,
}

impl Semantics {
    pub fn as_str(self) -> &'static str {
        match self {
            Semantics::Preserved => "preserved",
            Semantics::Corrected => "corrected",
            Semantics::Repaired => "repaired",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Semantics::Preserved => "behaviour is unchanged",
            Semantics::Corrected => "behaviour changes from self-contradictory to the code's evident meaning",
            Semantics::Repaired => "code that could not run as written now runs",
        }
    }
}

impl fmt::Display for Semantics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A 1-based line and column (columns count bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// A region of a source file.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start: Position,
    pub end: Position,
}

impl Span {
    /// A span from byte offsets into `text`.
    pub fn from_range(text: &str, start: usize, end: usize) -> Span {
        let pos = |at: usize| {
            let before = &text[..at.min(text.len())];
            let line = before.matches('\n').count() + 1;
            let column = at - before.rfind('\n').map(|i| i + 1).unwrap_or(0) + 1;
            Position { line, column }
        };
        Span {
            start_byte: start,
            end_byte: end,
            start: pos(start),
            end: pos(end),
        }
    }

    pub fn from_node(node: &tree_sitter::Node) -> Span {
        Span::from_nodes(node, node)
    }

    pub fn from_nodes(first: &tree_sitter::Node, last: &tree_sitter::Node) -> Span {
        let s = first.start_position();
        let e = last.end_position();
        Span {
            start_byte: first.start_byte(),
            end_byte: last.end_byte(),
            start: Position {
                line: s.row + 1,
                column: s.column + 1,
            },
            end: Position {
                line: e.row + 1,
                column: e.column + 1,
            },
        }
    }
}

/// A single text replacement. `start == end` is an insertion.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// A labelled location supporting a finding.
#[derive(Clone, Debug, Serialize)]
pub struct Evidence {
    pub span: Span,
    pub label: String,
}

/// A fact only a human can supply.
///
/// Most questions are yes/no: "yes" applies the correction, "no" records the
/// code as intentional. Others offer named options ("keep" / "remove",
/// "ours" / "theirs" / "both"), each saying what it means and whether it
/// changes the code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Question {
    pub id: String,
    pub text: String,
    pub options: Vec<QuestionOption>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct QuestionOption {
    pub answer: String,
    pub means: String,
    /// True when choosing it changes the code.
    pub changes: bool,
    /// Why this option cannot be applied here, if a guard refuses it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<String>,
}

impl Question {
    pub fn option(&self, answer: &str) -> Option<&QuestionOption> {
        self.options.iter().find(|o| o.answer.eq_ignore_ascii_case(answer))
    }

    /// "yes/no", "keep/remove"...
    pub fn choices(&self) -> String {
        self.options
            .iter()
            .map(|o| o.answer.as_str())
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// Where a finding stands.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Status {
    /// A proven correction is available and the engine may apply it.
    Fixable,
    /// A correction exists but depends on a fact only a human can supply.
    NeedsAnswer { question: Question },
    /// A human chose to keep the code as it is.
    Declined { question: Question, answer: String },
    /// No rule can prove a correction. The finding is explained only.
    Explained,
    /// A correction exists, but an engine guard refused to apply it.
    Refused { reason: String },
    /// A person chose not to hear about this finding here, with a reason:
    /// an inline `codemosis: ignore[...]` comment or a `[[suppress]]` entry.
    /// It is not counted, not corrected and not asked about.
    Suppressed { suppression: Suppression },
}

/// Where a suppression came from and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Suppression {
    /// "in-source" (a comment in the file) or "config" (`codemosis.toml`).
    pub kind: SuppressionKind,
    /// Why the finding is suppressed, in the author's words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The last day the suppression applies (YYYY-MM-DD).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Who answers for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Where it is written: `file:line` or `codemosis.toml`.
    pub at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuppressionKind {
    InSource,
    Config,
}

/// How far a finding's correction can be trusted, for reports and SARIF.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Every fact the rule needs holds; the engine may apply it.
    Proven,
    /// Proven given a fact a person supplied (an answer).
    ProvenGivenAnswer,
    /// A correction exists but needs a person's decision first.
    NeedsDecision,
    /// No correction can be proven; the finding is explained only.
    ManualOnly,
    /// A correction exists but a guard refused it here.
    Refused,
    /// Not applicable: the finding is declined or suppressed.
    None,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Proven => "proven",
            Confidence::ProvenGivenAnswer => "proven-given-answer",
            Confidence::NeedsDecision => "needs-decision",
            Confidence::ManualOnly => "manual-only",
            Confidence::Refused => "refused",
            Confidence::None => "none",
        }
    }
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Status::Fixable => "fixable",
            Status::NeedsAnswer { .. } => "needs answer",
            Status::Declined { .. } => "declined",
            Status::Explained => "explained",
            Status::Refused { .. } => "refused",
            Status::Suppressed { .. } => "suppressed",
        }
    }
}

/// One place where code falls short of its best form.
#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    /// Fully qualified rule id, e.g. `python-core/is-literal`.
    pub rule: String,
    pub title: String,
    pub category: Category,
    pub semantics: Option<Semantics>,
    /// Path relative to the project root.
    pub file: PathBuf,
    pub span: Span,
    pub message: String,
    pub evidence: Vec<Evidence>,
    /// Stable identity of this finding, independent of line numbers.
    pub anchor: String,
    pub status: Status,
    /// True when the correction was unlocked by a recorded answer.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub answered: bool,
    #[serde(skip)]
    pub edits: Vec<Edit>,
}

impl Finding {
    pub fn is_fixable(&self) -> bool {
        matches!(self.status, Status::Fixable)
    }

    /// Neither declined by a person nor suppressed: it counts.
    pub fn is_open(&self) -> bool {
        !matches!(self.status, Status::Declined { .. } | Status::Suppressed { .. })
    }

    pub fn confidence(&self) -> Confidence {
        match &self.status {
            Status::Fixable if self.answered => Confidence::ProvenGivenAnswer,
            Status::Fixable => Confidence::Proven,
            Status::NeedsAnswer { .. } => Confidence::NeedsDecision,
            Status::Explained => Confidence::ManualOnly,
            Status::Refused { .. } => Confidence::Refused,
            Status::Declined { .. } | Status::Suppressed { .. } => Confidence::None,
        }
    }

    /// A stable fingerprint: the same finding has the same fingerprint
    /// across runs, line shifts and machines. Used by baselines,
    /// suppression matching, SARIF and GitLab reports.
    pub fn fingerprint(&self) -> String {
        let file = self.file.to_string_lossy().replace('\\', "/");
        let key = format!("{}\u{0}{}\u{0}{}", self.rule, file, self.anchor);
        let mut h: u64 = 0xcbf29ce484222325;
        for b in key.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        let mut h2: u64 = 0x84222325cbf29ce4;
        for b in key.as_bytes().iter().rev() {
            h2 ^= *b as u64;
            h2 = h2.wrapping_mul(0x100000001b3);
        }
        format!("{h:016x}{h2:016x}")
    }

    pub fn question(&self) -> Option<&Question> {
        match &self.status {
            Status::NeedsAnswer { question } | Status::Declined { question, .. } => Some(question),
            _ => None,
        }
    }

    /// Identity used to recognise the same finding across rescans.
    pub fn key(&self) -> String {
        format!("{}\u{0}{}\u{0}{}", self.rule, self.file.display(), self.anchor)
    }
}

/// The measured distance between code and its best form: the number of open
/// findings in each category. Findings a human declined or suppressed are
/// not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Distance {
    pub broken: usize,
    pub contradiction: usize,
    pub hazard: usize,
    pub waste: usize,
    pub simplification: usize,
}

impl Distance {
    pub fn from_findings<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Distance {
        let mut d = Distance::default();
        for f in findings {
            if !f.is_open() {
                continue;
            }
            *d.slot(f.category) += 1;
        }
        d
    }

    fn slot(&mut self, c: Category) -> &mut usize {
        match c {
            Category::Broken => &mut self.broken,
            Category::Contradiction => &mut self.contradiction,
            Category::Hazard => &mut self.hazard,
            Category::Waste => &mut self.waste,
            Category::Simplification => &mut self.simplification,
        }
    }

    pub fn get(&self, c: Category) -> usize {
        match c {
            Category::Broken => self.broken,
            Category::Contradiction => self.contradiction,
            Category::Hazard => self.hazard,
            Category::Waste => self.waste,
            Category::Simplification => self.simplification,
        }
    }

    pub fn total(&self) -> usize {
        Category::ALL.iter().map(|c| self.get(*c)).sum()
    }
}

impl fmt::Display for Distance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parts: Vec<String> = Category::ALL
            .iter()
            .map(|c| format!("{} {}", c.as_str(), self.get(*c)))
            .collect();
        write!(f, "{}", parts.join(" · "))
    }
}
