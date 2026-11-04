//! The answers ledger: facts that only a human could supply, recorded in
//! `.codemosis/answers.toml` so every decision is reviewable and survives
//! between runs. Commit this file with the code it describes.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = ".codemosis/answers.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answer {
    pub id: String,
    pub rule: String,
    /// Where the question was asked, for the human reading the ledger.
    pub location: String,
    pub question: String,
    /// One of the question's answers: "yes" / "no", or an option such as
    /// "keep" or "remove".
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answers {
    #[serde(default, rename = "answer")]
    pub entries: Vec<Answer>,
}

impl Answers {
    pub fn path(root: &Path) -> PathBuf {
        root.join(FILE)
    }

    pub fn load(root: &Path) -> Result<Answers, String> {
        let path = Answers::path(root);
        if !path.is_file() {
            return Ok(Answers::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let answers: Answers = toml::from_str(&text).map_err(|e| format!("{} is not valid: {e}", path.display()))?;
        for a in &answers.entries {
            if a.answer.is_empty() || !a.answer.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                return Err(format!(
                    "{}: the answer to {} must be one of the question's answers (a lowercase word), found \"{}\"",
                    path.display(),
                    a.id,
                    a.answer
                ));
            }
        }
        Ok(answers)
    }

    pub fn save(&self, root: &Path) -> Result<(), String> {
        let path = Answers::path(root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        let body = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let text = format!(
            "# Facts supplied by people, recorded by Reverse Codemosis.\n\
             # Each answer is one of its question's options: an answer that changes code\n\
             # lets that correction be applied; one that keeps it records the code as intentional.\n\
             # Review this file like code, and commit it.\n\n{body}"
        );
        std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// The recorded answer to a question, if any.
    pub fn get(&self, id: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(id))
            .map(|a| a.answer.clone())
    }

    pub fn set(&mut self, answer: Answer) {
        self.entries.retain(|a| !a.id.eq_ignore_ascii_case(&answer.id));
        self.entries.push(answer);
        self.entries.sort_by(|a, b| a.id.cmp(&b.id));
    }
}
