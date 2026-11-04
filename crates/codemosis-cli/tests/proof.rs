//! Proof certificates end to end: `fix` writes one, `proof verify` replays
//! it, and any tampering with the code or the certificate is caught.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_codemosis"))
        .args(args)
        .args(["--color", "never"])
        .current_dir(dir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?}");
}

#[test]
fn a_certificate_replays_and_catches_tampering() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("demo");
    copy_dir(&workspace().join("examples/broken-inventory"), &dir);
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "broken"]);
    let out = run(&dir, &["fix", "."]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let certs: Vec<PathBuf> = fs::read_dir(dir.join(".codemosis/proofs"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(certs.len(), 1);
    let cert = certs[0].to_str().unwrap().to_string();

    let log = fs::read_to_string(dir.join(".codemosis/log.jsonl")).unwrap();
    assert!(
        log.contains("\"event\":\"fix\"") && log.contains(".codemosis/proofs/"),
        "{log}"
    );

    let out = run(&dir, &["proof", "verify", &cert]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(
        text.contains("Verified: the change is exactly what the pinned rules prove"),
        "{text}"
    );

    // The code changed after the certificate: not verified.
    let store = dir.join("inventory/store.py");
    let original = fs::read_to_string(&store).unwrap();
    fs::write(&store, format!("{original}\n# edited by hand\n")).unwrap();
    let out = run(&dir, &["proof", "verify", &cert]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("changed since the certificate was written"));
    fs::write(&store, original).unwrap();

    // The certificate was edited: not verified.
    let body = fs::read_to_string(&cert).unwrap();
    let forged = body.replacen("python-core/unused-import", "python-core/redundant-pass", 1);
    assert_ne!(forged, body);
    fs::write(&cert, forged).unwrap();
    let out = run(&dir, &["proof", "verify", &cert]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("digest"));
}
