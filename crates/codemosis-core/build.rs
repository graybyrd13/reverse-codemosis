//! Embeds into the binary, so `codemosis` works with no files beside it:
//!
//! * the packs in `/packs`;
//! * GitHub Linguist's catalogue of languages (`/data/linguist/languages.yml`),
//!   turned into a static table.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn toml_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn scalar(y: &yaml_rust2::Yaml) -> Option<String> {
    use yaml_rust2::Yaml;
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

fn strings(y: &yaml_rust2::Yaml) -> Vec<String> {
    y.as_vec()
        .map(|v| v.iter().filter_map(scalar).collect())
        .unwrap_or_default()
}

/// Turn Linguist's languages.yml into `LANGUAGES: &[LinguistLanguage]`.
fn linguist(manifest_dir: &Path) {
    use yaml_rust2::{Yaml, YamlLoader};
    let path = manifest_dir.join("../../data/linguist/languages.yml");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path).expect("data/linguist/languages.yml");
    let docs = YamlLoader::load_from_str(&text).expect("languages.yml is valid YAML");
    let root = docs[0].as_hash().expect("languages.yml maps names to languages");
    let mut out = String::from("pub static LANGUAGES: &[LinguistLanguage] = &[\n");
    for (name, lang) in root {
        let name = scalar(name).expect("language name");
        let get = |k: &str| lang[k].clone();
        let opt = |k: &str| match scalar(&get(k)) {
            Some(v) => format!("Some({v:?})"),
            None => "None".to_string(),
        };
        let kind = match scalar(&get("type")).as_deref() {
            Some("programming") => "Programming",
            Some("markup") => "Markup",
            Some("prose") => "Prose",
            _ => "Data",
        };
        let list = |k: &str| {
            let items: Vec<String> = strings(&get(k)).iter().map(|s| format!("{s:?}")).collect();
            format!("&[{}]", items.join(", "))
        };
        let id = match get("language_id") {
            Yaml::Integer(i) => i,
            _ => 0,
        };
        let _ = writeln!(
            out,
            "    LinguistLanguage {{ name: {name:?}, kind: Kind::{kind}, color: {}, extensions: {}, filenames: {}, interpreters: {}, aliases: {}, group: {}, ace_mode: {}, codemirror_mode: {}, tm_scope: {}, language_id: {id} }},",
            opt("color"),
            list("extensions"),
            list("filenames"),
            list("interpreters"),
            list("aliases"),
            opt("group"),
            opt("ace_mode"),
            opt("codemirror_mode"),
            opt("tm_scope"),
        );
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("linguist_data.rs");
    fs::write(dest, out).unwrap();
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    linguist(&manifest_dir);
    let packs = manifest_dir.join("../../packs");
    println!("cargo:rerun-if-changed={}", packs.display());

    let mut dirs: Vec<PathBuf> = fs::read_dir(&packs)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.join("pack.toml").is_file())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();

    let mut out = String::from("pub static BUILTIN_PACKS: &[BuiltinPack] = &[\n");
    for dir in dirs {
        let dir = dir.canonicalize().unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let _ = writeln!(
            out,
            "    BuiltinPack {{ dir: {name:?}, manifest: include_str!({:?}),",
            dir.join("pack.toml")
        );
        for (field, sub) in [("rules", "rules"), ("tables", "tables")] {
            let _ = write!(out, "        {field}: &[");
            for f in toml_files(&dir.join(sub)) {
                let rel = format!("{sub}/{}", f.file_name().unwrap().to_string_lossy());
                let _ = write!(out, "({rel:?}, include_str!({f:?})), ");
            }
            out.push_str("],\n");
        }
        out.push_str("    },\n");
    }
    out.push_str("];\n");
    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("builtin_packs.rs");
    fs::write(dest, out).unwrap();
}
