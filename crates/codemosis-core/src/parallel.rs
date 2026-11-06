//! Bounded parallelism with deterministic results.
//!
//! Reading, parsing and matching files are independent, so they run on all
//! cores; results come back in input order, so output never depends on
//! scheduling. `CODEMOSIS_THREADS=1` makes everything sequential.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// The number of worker threads: `$CODEMOSIS_THREADS`, or the machine's
/// available parallelism.
pub fn threads() -> usize {
    std::env::var("CODEMOSIS_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1))
}

/// `items.iter().map(f).collect()`, on several threads, in input order.
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = threads().min(items.len());
    if n <= 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                let r = f(&items[i]);
                *slots[i].lock().expect("no poisoned slot") = Some(r);
            });
        }
    });
    slots
        .into_iter()
        .map(|m| m.into_inner().expect("no poisoned slot").expect("every item mapped"))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_order() {
        let items: Vec<u64> = (0..1000).collect();
        let out = super::map(&items, |x| x * 2);
        assert_eq!(out, items.iter().map(|x| x * 2).collect::<Vec<_>>());
        assert!(super::map(&Vec::<u8>::new(), |x| *x).is_empty());
    }
}
