//! Diagnostic logging to standard error.
//!
//! Off by default. `--log debug` (or `CODEMOSIS_LOG=debug`) says what the
//! engine is doing: configuration, packs, files, oracle commands, each
//! correction step and why it was kept or rolled back. `--log trace` adds
//! per-file detail. `CODEMOSIS_LOG_FORMAT=json` writes one
//! JSON object per line, for log collectors.

use std::sync::atomic::{AtomicU8, Ordering};

static LEVEL: AtomicU8 = AtomicU8::new(0);
static JSON: AtomicU8 = AtomicU8::new(0);

pub const DEBUG: u8 = 1;
pub const TRACE: u8 = 2;

/// Set the level from flags (0, 1, 2), raised by `$CODEMOSIS_LOG` if set.
pub fn init(verbosity: u8) {
    let env = match std::env::var("CODEMOSIS_LOG").as_deref() {
        Ok("trace") => TRACE,
        Ok("debug") | Ok("info") => DEBUG,
        _ => 0,
    };
    LEVEL.store(verbosity.max(env).min(TRACE), Ordering::Relaxed);
    let json = std::env::var("CODEMOSIS_LOG_FORMAT").is_ok_and(|v| v == "json");
    JSON.store(u8::from(json), Ordering::Relaxed);
}

pub fn enabled(level: u8) -> bool {
    LEVEL.load(Ordering::Relaxed) >= level
}

fn clock() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let s = now.as_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        s / 3600,
        s / 60 % 60,
        s % 60,
        now.subsec_millis()
    )
}

pub fn emit(level: u8, target: &str, message: &str) {
    let name = if level >= TRACE { "trace" } else { "debug" };
    let target = target.trim_start_matches("codemosis_");
    let message = crate::redact::redact(message);
    if JSON.load(Ordering::Relaxed) == 1 {
        let escaped = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
        eprintln!(
            "{{\"time\":\"{}\",\"level\":\"{name}\",\"target\":\"{}\",\"message\":\"{}\"}}",
            clock(),
            escaped(target),
            escaped(&message)
        );
    } else {
        eprintln!("[{} {name} {target}] {message}", clock());
    }
}

/// Log at debug level: `codemosis_core::debug!("loaded {} packs", n)`.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::DEBUG) {
            $crate::log::emit($crate::log::DEBUG, module_path!(), &format!($($arg)*));
        }
    };
}

/// Log at trace level.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        if $crate::log::enabled($crate::log::TRACE) {
            $crate::log::emit($crate::log::TRACE, module_path!(), &format!($($arg)*));
        }
    };
}
