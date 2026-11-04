//! Pack data tables: facts about the world that rules look up, such as which
//! standard-library modules are free of import side effects.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Table {
    /// A set of strings.
    Set(BTreeSet<String>),
    /// A map from strings to lists of strings.
    Map(BTreeMap<String, Vec<String>>),
}

impl Table {
    pub fn has(&self, key: &str) -> bool {
        match self {
            Table::Set(s) => s.contains(key),
            Table::Map(m) => m.contains_key(key),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Table::Set(s) => s.len(),
            Table::Map(m) => m.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug, Default)]
pub struct Tables {
    pub tables: BTreeMap<String, Table>,
}

impl Tables {
    pub fn get(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    /// Read tables from a TOML document where every top-level key is a table:
    /// an array of strings becomes a set, a table of strings or string arrays
    /// becomes a map.
    pub fn merge_toml(&mut self, src: &str) -> Result<(), String> {
        let doc: toml::Table = toml::from_str(src).map_err(|e| e.to_string())?;
        for (name, value) in doc {
            let table = match value {
                toml::Value::Array(items) => {
                    let mut set = BTreeSet::new();
                    for item in items {
                        match item {
                            toml::Value::String(s) => {
                                set.insert(s);
                            }
                            other => {
                                return Err(format!("table '{name}': expected strings, found {}", other.type_str()))
                            }
                        }
                    }
                    Table::Set(set)
                }
                toml::Value::Table(entries) => {
                    let mut map = BTreeMap::new();
                    for (k, v) in entries {
                        let list = match v {
                            toml::Value::String(s) => vec![s],
                            toml::Value::Array(items) => items
                                .into_iter()
                                .map(|i| match i {
                                    toml::Value::String(s) => Ok(s),
                                    other => Err(format!(
                                        "table '{name}', key '{k}': expected strings, found {}",
                                        other.type_str()
                                    )),
                                })
                                .collect::<Result<Vec<_>, _>>()?,
                            other => {
                                return Err(format!(
                                    "table '{name}', key '{k}': expected a string or an array of strings, found {}",
                                    other.type_str()
                                ))
                            }
                        };
                        map.insert(k, list);
                    }
                    Table::Map(map)
                }
                other => {
                    return Err(format!(
                        "table '{name}': expected an array or a table, found {}",
                        other.type_str()
                    ))
                }
            };
            if self.tables.insert(name.clone(), table).is_some() {
                return Err(format!("table '{name}' is defined twice"));
            }
        }
        Ok(())
    }
}
