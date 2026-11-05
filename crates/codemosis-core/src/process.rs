//! Running external commands with a time limit and captured output.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct CommandResult {
    /// Exit code, or None when killed by a signal or the time limit.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub elapsed: Duration,
}

impl CommandResult {
    pub fn success(&self) -> bool {
        self.code == Some(0) && !self.timed_out
    }

    /// stdout and stderr together, for parsers that read both.
    pub fn combined(&self) -> String {
        let mut s = self.stdout.clone();
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&self.stderr);
        s
    }
}

/// Run `cmd`, killing it if it outlives `timeout`.
pub fn run(cmd: &mut Command, timeout: Duration) -> std::io::Result<CommandResult> {
    run_inner(cmd, None, timeout)
}

/// Run `cmd` with `input` on its standard input, killing it if it outlives
/// `timeout`. Formatters read code this way, so nothing is written to disk.
pub fn run_with_input(cmd: &mut Command, input: &str, timeout: Duration) -> std::io::Result<CommandResult> {
    run_inner(cmd, Some(input.as_bytes().to_vec()), timeout)
}

fn run_inner(cmd: &mut Command, input: Option<Vec<u8>>, timeout: Duration) -> std::io::Result<CommandResult> {
    let start = Instant::now();
    let mut child = cmd
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Feed standard input from a thread, so a child that writes before it
    // has read everything cannot deadlock against a full pipe.
    let in_thread = input.map(|bytes| {
        let mut pipe = child.stdin.take().expect("piped stdin");
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = pipe.write_all(&bytes);
        })
    });
    let mut out_pipe = child.stdout.take().expect("piped stdout");
    let mut err_pipe = child.stderr.take().expect("piped stderr");
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            break None;
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if let Some(t) = in_thread {
        let _ = t.join();
    }
    let stdout = String::from_utf8_lossy(&out_thread.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
    Ok(CommandResult {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr,
        timed_out,
        elapsed: start.elapsed(),
    })
}

/// Environment variables a pack example may see. Everything else (API
/// tokens, cloud credentials, CI secrets) is removed before an example runs,
/// because pack examples are code written by whoever wrote the pack.
pub const SNIPPET_ENV_ALLOWLIST: &[&str] = &[
    // Finding and running the toolchains.
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TEMP",
    "TMP",
    "XDG_CACHE_HOME",
    // Windows needs these to start any process.
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "PROGRAMDATA",
    "LOCALAPPDATA",
    "APPDATA",
    "USERPROFILE",
    // Toolchain locations and caches (no credentials).
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "CARGO_HOME",
    "GOROOT",
    "GOPATH",
    "GOCACHE",
    "GOMODCACHE",
    "JAVA_HOME",
    "DEVELOPER_DIR",
    "SDKROOT",
];

/// A command for running a pack example: the environment is reduced to
/// [`SNIPPET_ENV_ALLOWLIST`], standard input is closed, and on Unix the
/// process may not write files larger than 64 MB or dump core. Pack examples
/// run with the user's permissions otherwise, so `codemosis pack test`
/// should only be run on packs one would run the tests of; `check` and `fix`
/// never execute pack code.
pub fn snippet_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut cmd = Command::new(program);
    cmd.env_clear();
    for key in SNIPPET_ENV_ALLOWLIST {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    cmd.env("NO_COLOR", "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setrlimit is async-signal-safe and touches no memory the
        // parent shares; it only lowers limits of the child process.
        unsafe {
            cmd.pre_exec(|| {
                let core = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                libc::setrlimit(libc::RLIMIT_CORE, &core);
                let size = 64 * 1024 * 1024;
                let fsize = libc::rlimit {
                    rlim_cur: size,
                    rlim_max: size,
                };
                libc::setrlimit(libc::RLIMIT_FSIZE, &fsize);
                Ok(())
            });
        }
    }
    cmd
}

/// A shell command for the current platform.
pub fn shell(command: &str) -> Command {
    if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    }
}

/// A temporary directory removed when dropped.
pub struct ScratchDir {
    pub path: std::path::PathBuf,
}

impl ScratchDir {
    pub fn new(tag: &str) -> std::io::Result<ScratchDir> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("codemosis-{tag}-{}-{n}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(ScratchDir { path })
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn snippets_do_not_see_secrets() {
        std::env::set_var("CODEMOSIS_TEST_SECRET_TOKEN", "hunter2");
        let mut cmd = snippet_command("sh");
        cmd.arg("-c")
            .arg("echo \"[$CODEMOSIS_TEST_SECRET_TOKEN]\"; echo \"$PATH\" | head -c 1");
        let r = run(&mut cmd, Duration::from_secs(10)).unwrap();
        assert!(r.success(), "{r:?}");
        assert!(r.stdout.starts_with("[]\n"), "the secret leaked: {}", r.stdout);
        assert!(r.stdout.len() > 3, "PATH should survive: {}", r.stdout);
    }

    #[cfg(unix)]
    #[test]
    fn snippets_cannot_fill_the_disk() {
        let dir = ScratchDir::new("fsize").unwrap();
        let mut cmd = snippet_command("sh");
        cmd.arg("-c")
            .arg("head -c 100000000 /dev/zero > big; echo $?")
            .current_dir(&dir.path);
        let r = run(&mut cmd, Duration::from_secs(30)).unwrap();
        let size = std::fs::metadata(dir.path.join("big")).map(|m| m.len()).unwrap_or(0);
        assert!(size <= 64 * 1024 * 1024, "wrote {size} bytes");
        drop(r);
    }

    #[cfg(unix)]
    #[test]
    fn input_reaches_the_child_without_deadlock() {
        // Larger than any pipe buffer, echoed back while still being written.
        let input = "line of code\n".repeat(50_000);
        let r = run_with_input(&mut Command::new("cat"), &input, Duration::from_secs(30)).unwrap();
        assert!(r.success(), "{r:?}");
        assert_eq!(r.stdout, input);
    }
}
