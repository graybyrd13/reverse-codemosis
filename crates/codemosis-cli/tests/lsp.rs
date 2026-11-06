//! The language server end to end: JSON-RPC over the real binary's stdio.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, ChildStdout, Command, Stdio};

struct Client {
    stdin: ChildStdin,
    stdout: std::sync::mpsc::Receiver<Value>,
    /// Messages read while waiting for something else.
    queue: std::collections::VecDeque<Value>,
    next: i64,
}

/// Read framed JSON-RPC messages on a thread, so the test can time out
/// instead of hanging if the server stops answering.
fn reader(stdout: ChildStdout) -> std::sync::mpsc::Receiver<Value> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut r = BufReader::new(stdout);
        loop {
            let mut len = 0;
            loop {
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(n) = line.strip_prefix("Content-Length: ") {
                    len = n.parse().unwrap();
                }
            }
            let mut buf = vec![0; len];
            if r.read_exact(&mut buf).is_err() {
                return;
            }
            if tx.send(serde_json::from_slice(&buf).unwrap()).is_err() {
                return;
            }
        }
    });
    rx
}

impl Client {
    fn send(&mut self, msg: Value) {
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        self.stdout
            .recv_timeout(std::time::Duration::from_secs(30))
            .expect("the server answered within 30 s")
    }

    /// The first message (queued or new) that satisfies `want`.
    fn wait(&mut self, want: impl Fn(&Value) -> bool) -> Value {
        if let Some(i) = self.queue.iter().position(&want) {
            return self.queue.remove(i).unwrap();
        }
        loop {
            let m = self.read();
            if want(&m) {
                return m;
            }
            self.queue.push_back(m);
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.wait(|m| m["id"] == id && m.get("method").is_none())
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn diagnostics(&mut self, uri: &str) -> Vec<Value> {
        let m = self.wait(|m| m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri);
        m["params"]["diagnostics"].as_array().unwrap().clone()
    }
}

#[test]
fn diagnostics_fixes_hover_and_answers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let code = "import time\n\n\ndef f(x):\n    time.sleep(5)\n    return x is 5\n";
    std::fs::write(root.join("a.py"), code).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .arg("lsp")
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut c = Client {
        stdin: child.stdin.take().unwrap(),
        stdout: reader(child.stdout.take().unwrap()),
        queue: Default::default(),
        next: 0,
    };
    let root_uri = format!("file://{}", root.display());
    let uri = format!("{root_uri}/a.py");
    let init = c.request(
        "initialize",
        json!({"processId": null, "rootUri": root_uri, "capabilities": {}}),
    );
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true, "{init}");
    c.notify("initialized", json!({}));
    c.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri, "languageId": "python", "version": 1, "text": code}}),
    );
    let diags = c.diagnostics(&uri);
    let codes: Vec<&str> = diags.iter().map(|d| d["code"].as_str().unwrap()).collect();
    assert!(codes.contains(&"python-core/is-literal"), "{codes:?}");
    assert!(codes.contains(&"python-core/fixed-wait"), "{codes:?}");
    let literal = diags.iter().find(|d| d["code"] == "python-core/is-literal").unwrap();
    assert_eq!(literal["range"]["start"]["line"], 5);
    assert_eq!(literal["severity"], 1);

    // The proven correction is a quick fix.
    let actions = c.request(
        "textDocument/codeAction",
        json!({"textDocument": {"uri": uri}, "range": literal["range"], "context": {"diagnostics": []}}),
    );
    let fix = actions["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["kind"] == "quickfix" && a["title"].as_str().unwrap().starts_with("Correct:"))
        .expect("a quick fix");
    assert_eq!(fix["edit"]["changes"][&uri][0]["newText"], "==");

    // Hover explains the rule.
    let hover = c.request(
        "textDocument/hover",
        json!({"textDocument": {"uri": uri}, "position": literal["range"]["start"]}),
    );
    assert!(hover["result"]["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("python-core/is-literal"));

    // Answering "keep" records the answer and the question goes away.
    let wait = diags.iter().find(|d| d["code"] == "python-core/fixed-wait").unwrap();
    let actions = c.request(
        "textDocument/codeAction",
        json!({"textDocument": {"uri": uri}, "range": wait["range"], "context": {"diagnostics": []}}),
    );
    let keep = actions["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"].as_str().unwrap().contains(" keep: "))
        .expect("a keep action")
        .clone();
    let cmd = &keep["command"];
    let done = c.request(
        "workspace/executeCommand",
        json!({"command": cmd["command"], "arguments": cmd["arguments"]}),
    );
    assert!(done.get("error").is_none(), "{done}");
    let answers = std::fs::read_to_string(root.join(".codemosis/answers.toml")).unwrap();
    assert!(answers.contains("answer = \"keep\""), "{answers}");

    let after = c.diagnostics(&uri);
    assert!(after.iter().all(|d| d["code"] != "python-core/fixed-wait"), "{after:?}");

    c.request("shutdown", Value::Null);
    c.notify("exit", Value::Null);
    drop(c);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("the server did not exit after shutdown and exit");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
