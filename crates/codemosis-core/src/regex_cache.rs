//! Regular expressions compiled once per process.
//!
//! Facts such as `matches(text, pattern)` and Linguist's content heuristics
//! take their patterns as data, and run once per match or per file. Compiling
//! a regular expression costs far more than running it, so each distinct
//! pattern is compiled once and shared.

use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// The compiled form of `pattern`, compiling it on first use.
pub fn get(pattern: &str) -> Result<Arc<Regex>, regex::Error> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<Regex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(re) = cache.lock().expect("regex cache lock").get(pattern) {
        return Ok(re.clone());
    }
    let re = Arc::new(Regex::new(pattern)?);
    cache
        .lock()
        .expect("regex cache lock")
        .insert(pattern.to_string(), re.clone());
    Ok(re)
}
