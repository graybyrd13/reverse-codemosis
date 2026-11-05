//! Invariants of the built-in packs' tables that rules rely on instead of
//! repeating a check in every match. A rule that drops a fact because a table
//! makes it redundant names the test here that keeps the table so.

use codemosis_core::pack;
use codemosis_core::tables::Table;

fn table(pack_id: &str, name: &str) -> Table {
    let packs = pack::builtin().expect("built-in packs compile");
    let p = packs
        .iter()
        .find(|p| p.meta.id == pack_id)
        .unwrap_or_else(|| panic!("no pack {pack_id}"));
    p.tables
        .get(name)
        .unwrap_or_else(|| panic!("{pack_id} has no table {name}"))
        .clone()
}

fn keys(t: &Table) -> Vec<String> {
    match t {
        Table::Set(s) => s.iter().cloned().collect(),
        Table::Map(m) => m.keys().cloned().collect(),
    }
}

/// python-core's import rules do not check `!py.builtin(...)`: a name in
/// these tables is never a builtin, so adding its import cannot shadow one.
#[test]
fn stdlib_tables_hold_no_builtin_names() {
    let builtins = codemosis_core::lang::python::BUILTINS;
    for name in ["stdlib_names", "stdlib_modules"] {
        let clash: Vec<String> = keys(&table("python-core", name))
            .into_iter()
            .filter(|k| builtins.contains(&k.as_str()))
            .collect();
        assert!(clash.is_empty(), "{name} lists builtins: {clash:?}");
    }
}

/// waits/fixed-wait does not check `occupies_lines(@match)`: every pattern
/// matches whole lines, so deleting the match's lines deletes nothing else.
#[test]
fn wait_patterns_match_whole_lines() {
    let Table::Map(patterns) = table("waits", "wait_patterns") else {
        panic!("wait_patterns should map languages to patterns");
    };
    for (language, list) in &patterns {
        for p in list {
            let flags_end = if p.starts_with("(?") {
                p.find(')').map(|i| i + 1).unwrap_or(0)
            } else {
                0
            };
            let (flags, body) = p.split_at(flags_end);
            assert!(
                flags.contains('m') && body.starts_with('^') && body.ends_with('$'),
                "{language}: `{p}` must match whole lines: multi-line mode, ^ and $"
            );
        }
    }
}
