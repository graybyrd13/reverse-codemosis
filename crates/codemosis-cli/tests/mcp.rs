//! The MCP server end to end: a client speaks JSON-RPC to `codemosis mcp`
//! over standard input and output, on a copy of the broken-inventory demo.

use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap().flatten() {
        let p = entry.path();
        if entry.file_name() == "__pycache__" {
            continue;
        }
        if p.is_dir() {
            copy_dir(&p, &dst.join(entry.file_name()));
        } else {
            fs::copy(&p, dst.join(entry.file_name())).unwrap();
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

struct Client {
    child: Child,
    out: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start(root: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codemosis"))
            .args(["mcp", "--path"])
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .spawn()
            .expect("start codemosis mcp");
        let out = BufReader::new(child.stdout.take().unwrap());
        Client { child, out, next: 0 }
    }

    fn send(&mut self, msg: &Value) {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], id);
        reply
    }

    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.request("tools/call", json!({"name": tool, "arguments": args}));
        assert!(r.get("error").is_none(), "{r}");
        r["result"].clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.wait();
    }
}

fn text(result: &Value) -> &str {
    result["content"][0]["text"].as_str().unwrap_or("")
}

#[test]
fn protocol_and_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let demo = tmp.path().join("demo");
    copy_dir(&workspace().join("examples/broken-inventory"), &demo);
    let mut c = Client::start(&demo);

    // Version negotiation: a version we speak is echoed, another gets ours.
    let init = c.request(
        "initialize",
        json!({"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(init["result"]["serverInfo"]["name"], "codemosis");
    assert!(init["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("never answer on their behalf"));
    c.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let again = c.request("initialize", json!({"protocolVersion": "1999-01-01"}));
    assert_eq!(again["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));

    let tools = c.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap().clone();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        ["check", "fix", "answer", "explain", "map", "rules", "verify_proof"]
    );
    for t in &tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
        assert!(t["description"].as_str().unwrap().len() > 40, "{t}");
    }
    let answer = tools.iter().find(|t| t["name"] == "answer").unwrap();
    assert!(answer["description"].as_str().unwrap().contains("never yours"));

    // check: proven corrections and questions, as text and as structure.
    let r = c.call("check", json!({}));
    assert_eq!(r["isError"], false);
    let s = &r["structuredContent"];
    assert!(s["summary"]["fixable"].as_u64().unwrap() > 0, "{s}");
    assert!(s["summary"]["questions"].as_u64().unwrap() > 0, "{s}");
    assert!(text(&r).contains("Ask the user:"), "{}", text(&r));
    let question = s["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["status"] == "question")
        .unwrap()
        .clone();
    assert!(question["question"]["options"].as_array().unwrap().len() >= 2);

    // Scoped to one file.
    let r = c.call("check", json!({"files": ["inventory/pricing.py"]}));
    let files: Vec<&str> = r["structuredContent"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert!(
        !files.is_empty() && files.iter().all(|f| *f == "inventory/pricing.py"),
        "{files:?}"
    );

    // fix previews by default and changes nothing.
    let before = fs::read_to_string(demo.join("inventory/store.py")).unwrap();
    let r = c.call("fix", json!({}));
    assert_eq!(r["isError"], false, "{}", text(&r));
    assert!(
        text(&r).starts_with("Planned (preview, nothing written)"),
        "{}",
        text(&r)
    );
    assert!(text(&r).contains("```diff"));
    assert_eq!(fs::read_to_string(demo.join("inventory/store.py")).unwrap(), before);

    // Writing needs git; outside it, the tool says so instead of writing.
    let r = c.call("fix", json!({"apply": true}));
    assert_eq!(r["isError"], true);
    assert!(text(&r).contains("not in a git repository"), "{}", text(&r));

    // The other tools.
    let r = c.call("explain", json!({"rule": "python-core/is-literal"}));
    assert!(text(&r).contains("Identity comparison with a literal"));
    let r = c.call("explain", json!({"rule": "no-such/rule"}));
    assert_eq!(r["isError"], true);
    let r = c.call("rules", json!({"query": "fixed-wait"}));
    assert!(r["structuredContent"]["rules"].as_array().unwrap().len() > 10);
    let r = c.call("map", json!({}));
    assert!(text(&r).contains("Python"), "{}", text(&r));
    let r = c.call("answer", json!({"question_id": "Q-00000000", "answer": "keep"}));
    assert_eq!(r["isError"], true);

    // The user's answer, recorded: the question is no longer open.
    let qid = question["question"]["id"].as_str().unwrap().to_string();
    let keep = question["question"]["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["changes_code"] == false)
        .map(|o| o["answer"].as_str().unwrap().to_string())
        .unwrap();
    let r = c.call(
        "answer",
        json!({"question_id": qid, "answer": keep, "note": "the user said so"}),
    );
    assert_eq!(r["isError"], false, "{}", text(&r));
    let r = c.call("check", json!({}));
    assert!(!r["structuredContent"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["status"] == "question" && f["question"]["id"] == qid));

    // Protocol errors.
    let r = c.request("tools/call", json!({"name": "nope", "arguments": {}}));
    assert_eq!(r["error"]["code"], -32602);
    let r = c.request("no/such/method", json!({}));
    assert_eq!(r["error"]["code"], -32601);
}

#[test]
fn fix_applies_verifies_and_proves() {
    let have_python = Command::new("python3").arg("--version").output().is_ok();
    if !have_python || !Command::new("git").arg("--version").output().is_ok() {
        eprintln!("python3 or git missing; skipped");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let demo = tmp.path().join("demo");
    copy_dir(&workspace().join("examples/broken-inventory"), &demo);
    assert!(git(&demo, &["init", "-q"]) && git(&demo, &["add", "."]) && git(&demo, &["commit", "-q", "-m", "x"]));
    let mut c = Client::start(&demo);
    c.request("initialize", json!({"protocolVersion": "2025-06-18"}));

    let r = c.call("fix", json!({"apply": true}));
    assert_eq!(r["isError"], false, "{}", text(&r));
    assert!(text(&r).starts_with("Applied: "), "{}", text(&r));
    assert!(text(&r).contains("passing)"), "verified by the tests: {}", text(&r));
    let cert = r["structuredContent"]["certificate"].as_str().unwrap().to_string();

    let r = c.call("verify_proof", json!({"certificate": cert}));
    assert_eq!(r["structuredContent"]["verified"], true, "{}", text(&r));

    // A second apply on the now-dirty tree needs consent.
    let r = c.call("fix", json!({"apply": true}));
    assert_eq!(r["isError"], true);
    assert!(text(&r).contains("allow_dirty"), "{}", text(&r));
}
