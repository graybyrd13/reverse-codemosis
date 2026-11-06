//! `.codemosis/log.jsonl`: an append-only record of every decision that
//! changed what the engine does to a project: corrections applied (with
//! their proof certificate), answers recorded, baselines and locks written.
//! One JSON object per line, so it diffs and merges cleanly in git.

use serde_json::{json, Value};
use std::path::Path;

pub const AUDIT_LOG: &str = ".codemosis/log.jsonl";

/// Append one event. Failure to write the log never blocks the action it
/// records; the error is returned for the caller to report.
pub fn record(root: &Path, event: &str, mut data: Value) -> std::io::Result<()> {
    let mut entry = json!({
        "time": crate::proof::now_utc(),
        "event": event,
        "engine": codemosis_core::version::ENGINE_VERSION,
    });
    if let (Some(e), Some(d)) = (entry.as_object_mut(), data.as_object_mut()) {
        e.append(d);
    }
    crate::fsguard::append_confined(root, Path::new(AUDIT_LOG), &entry.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_json_lines() {
        let dir = tempfile::tempdir().unwrap();
        record(dir.path(), "answer", json!({"id": "Q-1", "answer": "yes"})).unwrap();
        record(dir.path(), "lock", json!({"packs": 3})).unwrap();
        let text = std::fs::read_to_string(dir.path().join(AUDIT_LOG)).unwrap();
        let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["event"], "answer");
        assert_eq!(lines[0]["answer"], "yes");
        assert_eq!(lines[1]["packs"], 3);
    }
}
